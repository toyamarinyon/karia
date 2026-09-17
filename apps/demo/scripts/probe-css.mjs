import { getCSSLanguageService, newCSSDataProvider } from 'vscode-css-languageservice';
import { TextDocument } from 'vscode-languageserver-textdocument';
import { readFile, writeFile, stat, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const version = require('vscode-css-languageservice/package.json').version;
const uri = name => pathToFileURL(resolve('src', name)).href;
const context = { resolveReference: (ref, base) => new URL(ref, base).href };
const fsProvider = {
  stat: async url => { const s = await stat(fileURLToPath(url)); return { type: s.isDirectory() ? 2 : 1, ctime: s.ctimeMs, mtime: s.mtimeMs, size: s.size }; },
  readDirectory: async url => (await readdir(fileURLToPath(url), { withFileTypes: true })).map(e => [e.name, e.isDirectory() ? 2 : 1]),
};
const makeService = () => getCSSLanguageService({ fileSystemProvider: fsProvider });
const results = [];
function document(text, name = 'probe.module.css') { return TextDocument.create(uri(name), 'css', 1, text); }
async function complete(service, marked, name) {
  const offset = marked.indexOf('|');
  if (offset < 0) throw Error('Missing cursor marker');
  const doc = document(marked.replace('|', ''), name);
  const ast = service.parseStylesheet(doc);
  const list = await service.doComplete2(doc, doc.positionAt(offset), ast, context);
  return list.items;
}
const labels = items => items.map(i => i.label);
const vars = items => items.filter(i => i.label.startsWith('--')).map(i => ({ label: i.label, documentation: i.documentation, textEdit: i.textEdit }));
async function recordCompletion(name, service, marked) {
  const items = await complete(service, marked);
  results.push({ name, variables: vars(items), totalItems: items.length });
}
const tokens = document(await readFile('src/tokens.css', 'utf8'), 'tokens.css');
const service = makeService();
await recordCompletion('same-file', service, ':root { --local-accent: #285a43; } .card { color: var(--|); }');
// Parsing another document does not imply that the service indexes a workspace.
service.parseStylesheet(tokens);
await recordCompletion('other-file-already-parsed', service, '.card { color: var(--|); }');
await recordCompletion('explicit-import', service, '@import "./tokens.css"; .card { color: var(--|); }');
const dataService = makeService();
dataService.setDataProviders(true, [newCSSDataProvider({ version: 1.1, properties: [
  { name: '--catalog-color', description: 'Color from custom data', syntax: '<color>' },
] })]);
await recordCompletion('custom-data-var-value', dataService, '.card { color: var(--|); }');
const properties = await complete(dataService, '.card { --| }');
results.push({ name: 'custom-data-property-name', hasCatalogColor: labels(properties).includes('--catalog-color'), variables: vars(properties) });
const basic = await complete(service, '.card { dis| }');
results.push({ name: 'standard-property-completion', hasDisplay: labels(basic).includes('display') });
for (const [name, text, needle] of [
  ['same-file-navigation', ':root { --local-accent: #285a43; } .card { color: var(--local-accent); }', '--local-accent'],
  ['imported-navigation', '@import "./tokens.css"; .card { color: var(--surface); }', '--surface'],
]) {
  const doc = document(text); const ast = service.parseStylesheet(doc);
  const pos = doc.positionAt(text.lastIndexOf(needle) + 3);
  results.push({ name, definition: service.findDefinition(doc, pos, ast), hover: service.doHover(doc, pos, ast), references: service.findReferences(doc, pos, ast), links: await service.findDocumentLinks2(doc, ast, context) });
}
for (const [name, text] of [
  ['unknown-variable', '.card { color: var(--does-not-exist); }'],
  ['unknown-property', '.card { colro: red; }'],
  ['css-modules-global', ':global(body) { margin: 0; } .card { color: red; }'],
]) {
  const doc = document(text); const ast = service.parseStylesheet(doc);
  results.push({ name, diagnostics: service.doValidation(doc, ast) });
}
const report = { package: 'vscode-css-languageservice', version, generatedAt: new Date().toISOString(), note: 'Direct language-service API probes; not an editor integration or standalone LSP server.', results };
await writeFile('probe-results.json', JSON.stringify(report, null, 2) + '\n');
console.log(`vscode-css-languageservice ${version}`);
for (const r of results) console.log(r.name, JSON.stringify(r.variables?.map(v => v.label) ?? r.diagnostics?.map(d => d.message) ?? (r.definition !== undefined ? { definitionFound: r.definition !== null, importLinks: r.links.length } : r)));
