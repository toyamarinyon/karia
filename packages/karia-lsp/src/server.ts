#!/usr/bin/env node
import { createConnection, ProposedFeatures, TextDocuments, TextDocumentSyncKind, CompletionItemKind, MarkupKind, DiagnosticSeverity } from 'vscode-languageserver/node.js';
import type { CompletionItem, Hover, Location, Range, Position, Diagnostic } from 'vscode-languageserver/node.js';
import { TextDocument } from 'vscode-languageserver-textdocument';
import { getCSSLanguageService } from 'vscode-css-languageservice';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import fs from 'node:fs/promises';
import { watch, type FSWatcher } from 'node:fs';
import path from 'node:path';
import ignore from 'ignore';
import { binaryPath } from 'karia-css';

type Definition = { name: string; insertionText: string; value: string; uri: string; start: number; end: number; context: string };
type CssContext = { variable: { name: string; start: number; end: number } | null; completion: { prefix: string; start: number; end: number } | null };
type ModuleAccess = { specifier: string; name: string; start: number; end: number };
type CoreDiagnostic = { uri: string; message: string; start: number; end: number; code: string };
const connection = createConnection(ProposedFeatures.all);
const documents = new TextDocuments(TextDocument);
const snapshots = new Map<string, string>();
const service = getCSSLanguageService();
let core: RustClient;
const watchers: FSWatcher[] = [];
let queue: Promise<unknown> = Promise.resolve();
function serial<T>(work: () => Promise<T>): Promise<T> {
  const result = queue.then(work);
  queue = result.catch(error => connection.console.error(String(error)));
  return result;
}
class RustClient {
  private child;
  private sequence = 0;
  private stopped: Error | undefined;
  private pending = new Map<number, { resolve: (value: unknown) => void; reject: (error: Error) => void; timer: NodeJS.Timeout }>();
  constructor() {
    this.child = spawn(binaryPath, ['serve'], { stdio: ['pipe', 'pipe', 'pipe'] });
    createInterface({ input: this.child.stdout }).on('line', line => {
      try {
        const response = JSON.parse(line);
        const pending = this.pending.get(response.id);
        if (!pending) return;
        clearTimeout(pending.timer); this.pending.delete(response.id);
        if (response.error) pending.reject(new Error(response.error)); else pending.resolve(response.result);
      } catch (error) { this.fail(new Error(`Invalid Rust response: ${String(error)}`)); }
    });
    this.child.stderr.on('data', data => connection.console.error(String(data)));
    this.child.on('error', error => this.fail(error));
    this.child.stdin.on('error', error => this.fail(error));
    this.child.on('exit', code => this.fail(new Error(`Rust worker exited (${code}); run npm run build before starting the LSP`)));
  }
  private fail(error: Error) {
    this.stopped = error;
    for (const pending of this.pending.values()) { clearTimeout(pending.timer); pending.reject(error); }
    this.pending.clear();
  }
  call<T>(method: string, params: Record<string, unknown> = {}): Promise<T> {
    if (this.stopped) return Promise.reject(this.stopped);
    return new Promise((resolve, reject) => {
      const id = ++this.sequence;
      const timer = setTimeout(() => { this.pending.delete(id); reject(new Error(`Rust ${method} timed out`)); }, 10000);
      this.pending.set(id, { resolve: value => resolve(value as T), reject, timer });
      this.child.stdin.write(JSON.stringify({ id, method, ...params }) + '\n');
    });
  }
  close() { this.child.stdin.end(); this.child.kill(); }
}
const isCss = (uri: string) => uri.startsWith('file:') && fileURLToPath(uri).endsWith('.css');
// Hard exclusions apply even when a .gitignore un-ignores them.
const ignoredDirectories = new Set(['node_modules', 'dist', 'target', '.git', '.turbo']);
const workspaceRoots: string[] = [];
// One matcher per .gitignore file, scoped to the directory containing it.
const ignoreRules: { base: string; matcher: ignore.Ignore }[] = [];
function isIgnored(abs: string, isDir = false): boolean {
  const root = workspaceRoots.find(r => abs === r || abs.startsWith(r + path.sep));
  const segments = (root ? path.relative(root, abs) : abs).split(path.sep);
  if (segments.some(part => ignoredDirectories.has(part))) return true;
  for (const { base, matcher } of ignoreRules) {
    const rel = path.relative(base, abs);
    if (!rel || rel.startsWith('..') || path.isAbsolute(rel)) continue;
    const pathname = rel.split(path.sep).join('/');
    if (matcher.ignores(isDir ? pathname + '/' : pathname)) return true;
  }
  return false;
}
async function loadGitignore(dir: string) {
  const existing = ignoreRules.findIndex(r => r.base === dir);
  try {
    const matcher = ignore().add(await fs.readFile(path.join(dir, '.gitignore'), 'utf8'));
    if (existing >= 0) ignoreRules[existing] = { base: dir, matcher };
    else ignoreRules.push({ base: dir, matcher });
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
    if (existing >= 0) ignoreRules.splice(existing, 1);
  }
}
async function refreshGitignore(dir: string) {
  if (isIgnored(dir, true)) return;
  await loadGitignore(dir);
  for (const uri of [...snapshots.keys()]) {
    // Open documents keep their unsaved overlay even when gitignored.
    if (isCss(uri) && !documents.get(uri) && isIgnored(fileURLToPath(uri))) {
      snapshots.delete(uri); await core.call('remove', { uri });
    }
  }
  await scanDirectory(dir);
  await publishDiagnostics();
}
async function indexFile(uri: string) {
  if (!isCss(uri)) return;
  const open = documents.get(uri);
  if (open) { await update(uri, open.getText()); return; }
  if (isIgnored(fileURLToPath(uri))) {
    snapshots.delete(uri); await core.call('remove', { uri });
    return;
  }
  try { await update(uri, await fs.readFile(fileURLToPath(uri), 'utf8')); }
  catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
    snapshots.delete(uri); await core.call('remove', { uri });
  }
}
async function update(uri: string, text: string) { snapshots.set(uri, text); await core.call('update', { uri, text }); }
async function scanDirectory(dir: string) {
  await loadGitignore(dir);
  for (const entry of await fs.readdir(dir, { withFileTypes: true })) {
    const file = path.join(dir, entry.name);
    if (isIgnored(file, entry.isDirectory())) continue;
    if (entry.isDirectory()) await scanDirectory(file);
    else if (entry.isFile() && entry.name.endsWith('.css')) await indexFile(pathToFileURL(file).href);
  }
}
function document(uri: string) { return documents.get(uri) ?? TextDocument.create(uri, 'css', 0, snapshots.get(uri) ?? ''); }
function bytePosition(uri: string, byte: number): Position {
  const doc = document(uri);
  const offset = Buffer.from(doc.getText()).subarray(0, byte).toString('utf8').length;
  return doc.positionAt(offset);
}
function byteRange(uri: string, start: number, end: number): Range { return { start: bytePosition(uri, start), end: bytePosition(uri, end) }; }
function location(definition: Definition): Location { return { uri: definition.uri, range: byteRange(definition.uri, definition.start, definition.end) }; }
function markdown(found: Definition[]): string {
  return found.map(d => `\`\`\`css\n${d.name.startsWith('--') ? `${d.name}: ${d.value};` : `.${d.name} {\n${d.value}\n}`}\n\`\`\`\n\nContext: ${d.context}\n\n[${path.basename(fileURLToPath(d.uri))}](${d.uri})`).join('\n\n---\n\n');
}
function cssContext(doc: TextDocument, offset: number) {
  // LSP document offsets are UTF-16; the indexed Rust source uses UTF-8 bytes.
  return core.call<CssContext | null>('css-context', {
    uri: doc.uri, offset: Buffer.byteLength(doc.getText().slice(0, offset), 'utf8'),
  });
}
async function moduleDefinitions(uri: string, specifier: string) {
  const target = pathToFileURL(path.resolve(path.dirname(fileURLToPath(uri)), specifier)).href;
  if (!snapshots.has(target)) await indexFile(target);
  return core.call<Definition[]>('classes', { uri: target });
}
// Only index-derived diagnostics are published; standard CSS lint is left to
// linters such as stylelint.
async function publishDiagnostics() {
  for (const doc of documents.all()) {
    if (!isCss(doc.uri)) continue;
    const extra = await core.call<CoreDiagnostic[]>('diagnostics', { uri: doc.uri });
    const diagnostics: Diagnostic[] = extra.map(d => ({ message: d.message, range: byteRange(doc.uri, d.start, d.end), severity: DiagnosticSeverity.Warning, code: d.code, source: 'karia' }));
    connection.sendDiagnostics({ uri: doc.uri, version: doc.version, diagnostics });
  }
}
connection.onInitialize(params => serial(async () => {
  core = new RustClient();
  // Fail initialization clearly if the mandatory Rust worker is unavailable.
  await core.call('variables');
  const roots = params.workspaceFolders?.map(f => f.uri) ?? (params.rootUri ? [params.rootUri] : []);
  for (const uri of roots) if (uri.startsWith('file:')) {
    const root = fileURLToPath(uri);
    workspaceRoots.push(root);
    await scanDirectory(root);
    const watcher = watch(root, { recursive: true }, (_event, filename) => {
      if (!filename) return;
      const abs = path.join(root, filename);
      if (path.basename(filename) === '.gitignore') {
        void serial(() => refreshGitignore(path.dirname(abs)));
        return;
      }
      if (!filename.endsWith('.css') || isIgnored(abs)) return;
      void serial(async () => { await indexFile(pathToFileURL(abs).href); await publishDiagnostics(); });
    });
    watcher.on('error', error => connection.console.error(`Workspace watcher: ${error.message}`));
    watchers.push(watcher);
  }
  return { capabilities: {
    textDocumentSync: TextDocumentSyncKind.Incremental,
    completionProvider: { triggerCharacters: ['.', '-', '(', '"', "'"] },
    hoverProvider: true, definitionProvider: true,
  }, serverInfo: { name: 'karia', version: '0.0.2' } };
}));
documents.onDidChangeContent(({ document: doc }) => {
  // Snapshot the event text before queuing: later edits may mutate document state.
  const text = doc.getText();
  void serial(async () => { if (isCss(doc.uri)) { await update(doc.uri, text); await publishDiagnostics(); } });
});
documents.onDidClose(({ document: doc }) => {
  void serial(async () => {
    if (isCss(doc.uri)) { await indexFile(doc.uri); await publishDiagnostics(); }
    connection.sendDiagnostics({ uri: doc.uri, diagnostics: [] });
  });
});
connection.onDidChangeWatchedFiles(params => {
  void serial(async () => {
    for (const change of params.changes) {
      if (path.basename(fileURLToPath(change.uri)) === '.gitignore') {
        await refreshGitignore(path.dirname(fileURLToPath(change.uri)));
      } else await indexFile(change.uri);
    }
    await publishDiagnostics();
  });
});
connection.onCompletion(params => serial(async (): Promise<CompletionItem[] | null> => {
  const doc = documents.get(params.textDocument.uri); if (!doc) return null;
  const text = doc.getText(), offset = doc.offsetAt(params.position);
  if (isCss(doc.uri)) {
    const completion = (await cssContext(doc, offset))?.completion;
    if (completion) {
      const { prefix } = completion;
      const found = await core.call<Definition[]>('variables');
      const groups = new Map<string, Definition[]>();
      for (const d of found) if (d.name.startsWith(prefix)) groups.set(d.name, [...(groups.get(d.name) ?? []), d]);
      return [...groups].map(([name, defs]) => ({ label: name, kind: CompletionItemKind.Variable,
        detail: defs.map(d => d.value).join(' | '), documentation: { kind: MarkupKind.Markdown, value: markdown(defs) },
        textEdit: { range: byteRange(doc.uri, completion.start, completion.end), newText: defs[0].insertionText } }));
    }
    return service.doComplete(doc, params.position, service.parseStylesheet(doc)).items;
  }
  const access = await core.call<ModuleAccess | null>('module-access', { text, offset });
  if (!access) return null;
  const found = await moduleDefinitions(doc.uri, access.specifier);
  return [...new Map(found.map(d => [d.name, d])).values()].map(d => ({ label: d.name, kind: CompletionItemKind.Field,
    documentation: { kind: MarkupKind.Markdown, value: markdown([d]) },
    // The worker reports UTF-8 byte offsets; bytePosition converts to LSP positions.
    textEdit: { range: { start: bytePosition(doc.uri, access.start), end: bytePosition(doc.uri, access.end) }, newText: d.name } }));
}));
connection.onHover(params => serial(async (): Promise<Hover | null> => {
  const doc = documents.get(params.textDocument.uri); if (!doc) return null;
  const text = doc.getText(), offset = doc.offsetAt(params.position);
  if (isCss(doc.uri)) {
    const token = (await cssContext(doc, offset))?.variable;
    if (token) {
      const found = await core.call<Definition[]>('inspect', { name: token.name });
      return found.length ? { contents: { kind: MarkupKind.Markdown, value: markdown(found) }, range: byteRange(doc.uri, token.start, token.end) } : null;
    }
    return service.doHover(doc, params.position, service.parseStylesheet(doc));
  }
  const access = await core.call<ModuleAccess | null>('module-access', { text, offset });
  if (!access) return null;
  const found = (await moduleDefinitions(doc.uri, access.specifier)).filter(d => d.name === access.name);
  return found.length ? { contents: { kind: MarkupKind.Markdown, value: markdown(found) } } : null;
}));
connection.onDefinition(params => serial(async (): Promise<Location[] | Location | null> => {
  const doc = documents.get(params.textDocument.uri); if (!doc) return null;
  const text = doc.getText(), offset = doc.offsetAt(params.position);
  if (isCss(doc.uri)) {
    const token = (await cssContext(doc, offset))?.variable;
    if (token) return (await core.call<Definition[]>('inspect', { name: token.name })).map(location);
    return service.findDefinition(doc, params.position, service.parseStylesheet(doc));
  }
  const access = await core.call<ModuleAccess | null>('module-access', { text, offset });
  if (!access) return null;
  return (await moduleDefinitions(doc.uri, access.specifier)).filter(d => d.name === access.name).map(location);
}));
connection.onShutdown(() => { for (const watcher of watchers) watcher.close(); core?.close(); });
connection.onExit(() => { core?.close(); });
process.on('exit', () => core?.close());
documents.listen(connection);
connection.listen();
