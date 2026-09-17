#!/usr/bin/env node
import { createConnection, ProposedFeatures, TextDocuments, TextDocumentSyncKind, CompletionItemKind, MarkupKind, DiagnosticSeverity } from 'vscode-languageserver/node.js';
import type { CompletionItem, Hover, Location, Range, Position, Diagnostic } from 'vscode-languageserver/node.js';
import { TextDocument } from 'vscode-languageserver-textdocument';
import { getCSSLanguageService } from 'vscode-css-languageservice';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { createRequire } from 'node:module';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import fs from 'node:fs/promises';
import { watch, type FSWatcher } from 'node:fs';
import path from 'node:path';
import { moduleAccess } from './tsx.js';

type Definition = { name: string; value: string; uri: string; start: number; end: number; context: string };
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
    const require = createRequire(import.meta.url);
    const base = path.dirname(require.resolve('karia/package.json'));
    this.child = spawn(path.join(base, 'target/release', process.platform === 'win32' ? 'karia.exe' : 'karia'), ['serve'], { stdio: ['pipe', 'pipe', 'pipe'] });
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
async function indexFile(uri: string) {
  if (!isCss(uri)) return;
  const open = documents.get(uri);
  if (open) { await update(uri, open.getText()); return; }
  try { await update(uri, await fs.readFile(fileURLToPath(uri), 'utf8')); }
  catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
    snapshots.delete(uri); await core.call('remove', { uri });
  }
}
async function update(uri: string, text: string) { snapshots.set(uri, text); await core.call('update', { uri, text }); }
async function scanDirectory(dir: string) {
  for (const entry of await fs.readdir(dir, { withFileTypes: true })) {
    if (['node_modules', 'dist', 'target', '.git', '.turbo'].includes(entry.name)) continue;
    const file = path.join(dir, entry.name);
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
// This is cursor-context detection only; CSS symbols/values are parsed in Rust.
function inCommentOrString(text: string, offset: number): boolean {
  let quote = '', comment = false;
  for (let i = 0; i < offset; i++) {
    if (comment) { if (text[i] === '*' && text[i + 1] === '/') { comment = false; i++; } continue; }
    if (quote) { if (text[i] === '\\') i++; else if (text[i] === quote) quote = ''; continue; }
    if (text[i] === '/' && text[i + 1] === '*') { comment = true; i++; }
    else if (text[i] === '"' || text[i] === "'") quote = text[i];
  }
  return comment || !!quote;
}
function variableAt(text: string, offset: number) {
  if (inCommentOrString(text, offset)) return undefined;
  let start = offset, end = offset;
  const word = /[\p{L}\p{N}_-]/u;
  while (start > 0 && word.test(text[start - 1])) start--;
  while (end < text.length && word.test(text[end])) end++;
  const name = text.slice(start, end);
  return name.startsWith('--') ? { name, start, end } : undefined;
}
async function moduleDefinitions(uri: string, specifier: string) {
  const target = pathToFileURL(path.resolve(path.dirname(fileURLToPath(uri)), specifier)).href;
  if (!snapshots.has(target)) await indexFile(target);
  return core.call<Definition[]>('classes', { uri: target });
}
async function publishDiagnostics() {
  for (const doc of documents.all()) {
    if (!isCss(doc.uri)) continue;
    const diagnostics: Diagnostic[] = service.doValidation(doc, service.parseStylesheet(doc));
    const extra = await core.call<CoreDiagnostic[]>('diagnostics', { uri: doc.uri });
    diagnostics.push(...extra.map(d => ({ message: d.message, range: byteRange(doc.uri, d.start, d.end), severity: DiagnosticSeverity.Warning, code: d.code, source: 'css-lab' })));
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
    await scanDirectory(root);
    const watcher = watch(root, { recursive: true }, (_event, filename) => {
      if (!filename || !filename.endsWith('.css') || filename.split(path.sep).some(part => ['node_modules', 'dist', 'target', '.git', '.turbo'].includes(part))) return;
      void serial(async () => { await indexFile(pathToFileURL(path.join(root, filename)).href); await publishDiagnostics(); });
    });
    watcher.on('error', error => connection.console.error(`Workspace watcher: ${error.message}`));
    watchers.push(watcher);
  }
  return { capabilities: {
    textDocumentSync: TextDocumentSyncKind.Incremental,
    completionProvider: { triggerCharacters: ['.', '-', '(', '"', "'"] },
    hoverProvider: true, definitionProvider: true,
  }, serverInfo: { name: 'karia', version: '0.0.0' } };
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
    for (const change of params.changes) await indexFile(change.uri);
    await publishDiagnostics();
  });
});
connection.onCompletion(params => serial(async (): Promise<CompletionItem[] | null> => {
  const doc = documents.get(params.textDocument.uri); if (!doc) return null;
  const text = doc.getText(), offset = doc.offsetAt(params.position);
  if (isCss(doc.uri)) {
    const before = text.slice(0, offset);
    const match = /var\(\s*(--[\p{L}\p{N}_-]*)?$/u.exec(before);
    if (match && !inCommentOrString(text, offset)) {
      const prefix = match[1] ?? '';
      const found = await core.call<Definition[]>('variables');
      const groups = new Map<string, Definition[]>();
      for (const d of found) if (d.name.startsWith(prefix)) groups.set(d.name, [...(groups.get(d.name) ?? []), d]);
      return [...groups].map(([name, defs]) => ({ label: name, kind: CompletionItemKind.Variable,
        detail: defs.map(d => d.value).join(' | '), documentation: { kind: MarkupKind.Markdown, value: markdown(defs) },
        textEdit: { range: { start: doc.positionAt(offset - prefix.length), end: doc.positionAt(variableAt(text, offset)?.end ?? offset) }, newText: name } }));
    }
    return service.doComplete(doc, params.position, service.parseStylesheet(doc)).items;
  }
  const access = moduleAccess(text, offset); if (!access) return null;
  const found = await moduleDefinitions(doc.uri, access.specifier);
  return [...new Map(found.map(d => [d.name, d])).values()].map(d => ({ label: d.name, kind: CompletionItemKind.Field,
    documentation: { kind: MarkupKind.Markdown, value: markdown([d]) },
    textEdit: { range: { start: doc.positionAt(access.start), end: doc.positionAt(access.end) }, newText: d.name } }));
}));
connection.onHover(params => serial(async (): Promise<Hover | null> => {
  const doc = documents.get(params.textDocument.uri); if (!doc) return null;
  const text = doc.getText(), offset = doc.offsetAt(params.position);
  if (isCss(doc.uri)) {
    const token = variableAt(text, offset);
    if (token) {
      const found = await core.call<Definition[]>('inspect', { name: token.name });
      return found.length ? { contents: { kind: MarkupKind.Markdown, value: markdown(found) }, range: { start: doc.positionAt(token.start), end: doc.positionAt(token.end) } } : null;
    }
    return service.doHover(doc, params.position, service.parseStylesheet(doc));
  }
  const access = moduleAccess(text, offset); if (!access) return null;
  const found = (await moduleDefinitions(doc.uri, access.specifier)).filter(d => d.name === access.name);
  return found.length ? { contents: { kind: MarkupKind.Markdown, value: markdown(found) } } : null;
}));
connection.onDefinition(params => serial(async (): Promise<Location[] | Location | null> => {
  const doc = documents.get(params.textDocument.uri); if (!doc) return null;
  const text = doc.getText(), offset = doc.offsetAt(params.position);
  if (isCss(doc.uri)) {
    const token = variableAt(text, offset);
    if (token) return (await core.call<Definition[]>('inspect', { name: token.name })).map(location);
    return service.findDefinition(doc, params.position, service.parseStylesheet(doc));
  }
  const access = moduleAccess(text, offset); if (!access) return null;
  return (await moduleDefinitions(doc.uri, access.specifier)).filter(d => d.name === access.name).map(location);
}));
connection.onShutdown(() => { for (const watcher of watchers) watcher.close(); core?.close(); });
connection.onExit(() => { core?.close(); });
process.on('exit', () => core?.close());
documents.listen(connection);
connection.listen();
