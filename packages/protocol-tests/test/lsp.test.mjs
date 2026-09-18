import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';
const require = createRequire(import.meta.url);
const server = join(dirname(require.resolve('karia-lsp/package.json')), 'dist/server.js');

function client() {
  const proc = spawn(process.execPath, [server, '--stdio'], { stdio: ['pipe', 'pipe', 'pipe'] });
  let buffer = Buffer.alloc(0), sequence = 0, stderr = '';
  const pending = new Map();
  proc.stderr.on('data', chunk => { stderr += chunk; });
  function send(message) {
    const body = JSON.stringify({ jsonrpc: '2.0', ...message });
    proc.stdin.write(`Content-Length: ${Buffer.byteLength(body)}\r\n\r\n${body}`);
  }
  proc.stdout.on('data', chunk => {
    buffer = Buffer.concat([buffer, chunk]);
    while (true) {
      const split = buffer.indexOf('\r\n\r\n');
      if (split < 0) break;
      const length = Number(/Content-Length:\s*(\d+)/i.exec(buffer.subarray(0, split).toString())?.[1]);
      if (!Number.isFinite(length)) throw new Error('Invalid LSP header');
      if (buffer.length < split + 4 + length) break;
      const msg = JSON.parse(buffer.subarray(split + 4, split + 4 + length).toString());
      buffer = buffer.subarray(split + 4 + length);
      if (msg.method && msg.id != null) { send({ id: msg.id, result: null }); continue; }
      if (pending.has(msg.id)) {
        const { resolve, reject, timer } = pending.get(msg.id);
        clearTimeout(timer); pending.delete(msg.id);
        if (msg.error) reject(new Error(JSON.stringify(msg.error))); else resolve(msg.result);
      }
    }
  });
  proc.on('exit', code => {
    for (const p of pending.values()) { clearTimeout(p.timer); p.reject(new Error(`LSP exited ${code}: ${stderr}`)); }
    pending.clear();
  });
  return {
    notify: (method, params) => send({ method, params }),
    request: (method, params) => new Promise((resolve, reject) => {
      const id = ++sequence;
      const timer = setTimeout(() => { pending.delete(id); reject(new Error(`${method} timeout: ${stderr}`)); }, 15000);
      pending.set(id, { resolve, reject, timer }); send({ id, method, params });
    }),
    async close() {
      try { await this.request('shutdown', null); this.notify('exit'); } finally { proc.stdin.end(); proc.kill(); }
    },
  };
}
function pos(text, needle, delta = 0) {
  const offset = text.indexOf(needle) + delta;
  assert.ok(offset >= 0, `missing ${needle}`);
  const lines = text.slice(0, offset).split('\n');
  return { line: lines.length - 1, character: lines.at(-1).length };
}
function items(result) { return Array.isArray(result) ? result : result.items; }
function locations(result) { return result == null ? [] : Array.isArray(result) ? result : [result]; }

 test('real stdio LSP: CSS service + Rust index + unsaved CSS Modules', { timeout: 90000 }, async t => {
  const root = await mkdtemp(join(tmpdir(), 'css-lab-'));
  const tokens = ':root { --accent: #285a43; --space: 24px; }\n[data-theme="dark"] { --accent: #99ccaa; }';
  const css = '/* 日本語 🌿 */\n.card { background: var(--accent); padding: var(--space); }\n.button { color: white; }';
  const tsx = 'import styles from "./App.module.css";\nexport const view = <div className={styles.card} />;';
  const uri = name => pathToFileURL(join(root, name)).href;
  await Promise.all([writeFile(join(root, 'tokens.css'), tokens), writeFile(join(root, 'App.module.css'), css), writeFile(join(root, 'App.tsx'), tsx)]);
  const c = client();
  t.after(async () => { await c.close(); await rm(root, { recursive: true, force: true }); });
  const init = await c.request('initialize', { processId: process.pid, rootUri: pathToFileURL(root).href, capabilities: {}, workspaceFolders: [{ uri: pathToFileURL(root).href, name: 'fixture' }] });
  assert.ok(init.capabilities.hoverProvider);
  c.notify('initialized', {});
  const open = (name, text, languageId) => c.notify('textDocument/didOpen', { textDocument: { uri: uri(name), languageId, version: 1, text } });
  const change = (name, text, version = 2) => c.notify('textDocument/didChange', { textDocument: { uri: uri(name), version }, contentChanges: [{ text }] });
  const query = (method, name, text, needle, delta) => c.request(`textDocument/${method}`, { textDocument: { uri: uri(name) }, position: pos(text, needle, delta) });
  open('App.module.css', css, 'css'); open('App.tsx', tsx, 'typescriptreact');

  const completion = await query('completion', 'App.module.css', css, '--accent', 2);
  const accentItem = items(completion).find(x => x.label === '--accent');
  assert.ok(accentItem);
  assert.deepEqual(accentItem.textEdit.range.end, pos(css, '--accent', '--accent'.length), 'completion replaces the whole token, including suffix');
  const hover = await query('hover', 'App.module.css', css, '--accent', 4);
  assert.match(JSON.stringify(hover), /#285a43/);
  assert.match(JSON.stringify(hover), /#99ccaa/);
  assert.doesNotMatch(JSON.stringify(hover), /Shorthand property/);
  const defs = locations(await query('definition', 'App.module.css', css, '--accent', 4));
  assert.ok(defs.some(d => (d.uri ?? d.targetUri) === uri('tokens.css')));
  const normal = await query('hover', 'App.module.css', css, 'background', 3);
  assert.match(JSON.stringify(normal), /background|Shorthand/);

  const tsDefs = locations(await query('definition', 'App.tsx', tsx, 'styles.card', 9));
  assert.ok(tsDefs.some(d => (d.uri ?? d.targetUri) === uri('App.module.css')));
  assert.equal((tsDefs[0].range ?? tsDefs[0].targetSelectionRange).start.line, 1);
  const classHover = await query('hover', 'App.tsx', tsx, 'styles.card', 9);
  assert.match(JSON.stringify(classHover), /background/);
  const incomplete = 'import styles from "./App.module.css";\nstyles.';
  change('App.tsx', incomplete);
  const classes = items(await query('completion', 'App.tsx', incomplete, 'styles.', 7));
  assert.ok(classes.some(x => x.label === 'card'));
  assert.ok(classes.some(x => x.label === 'button'));

  open('tokens.css', tokens, 'css');
  change('tokens.css', ':root { --accent: hotpink; --fresh: 10px; }');
  const editedHover = await query('hover', 'App.module.css', css, '--accent', 4);
  assert.match(JSON.stringify(editedHover), /hotpink/);
  assert.doesNotMatch(JSON.stringify(editedHover), /#285a43/);
  const updatedCss = '/* 🌿 */ .newCard { color: red; }';
  change('App.module.css', updatedCss);
  const editedClasses = items(await query('completion', 'App.tsx', incomplete, 'styles.', 7));
  assert.ok(editedClasses.some(x => x.label === 'newCard'));
  assert.ok(!editedClasses.some(x => x.label === 'card'));

  c.notify('textDocument/didClose', { textDocument: { uri: uri('App.module.css') } });
  const restored = items(await query('completion', 'App.tsx', incomplete, 'styles.', 7));
  assert.ok(restored.some(x => x.label === 'card'));
  assert.ok(!restored.some(x => x.label === 'newCard'));

  await writeFile(join(root, 'added.css'), ':root { --added: cyan; }');
  c.notify('workspace/didChangeWatchedFiles', { changes: [{ uri: uri('added.css'), type: 1 }] });
  open('scratch.css', '.a { color: var(--); }', 'css');
  const all = items(await query('completion', 'scratch.css', '.a { color: var(--); }', '--', 2));
  assert.ok(all.some(x => x.label === '--added'));
  await rm(join(root, 'added.css'));
  c.notify('workspace/didChangeWatchedFiles', { changes: [{ uri: uri('added.css'), type: 3 }] });
  const removed = items(await query('completion', 'scratch.css', '.a { color: var(--); }', '--', 2));
  assert.ok(!removed.some(x => x.label === '--added'));
 });
