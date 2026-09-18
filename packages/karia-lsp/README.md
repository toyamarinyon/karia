# karia-lsp

CSS language server for CSS Modules and CSS custom properties, backed by the
[karia-css](https://www.npmjs.com/package/karia-css) Rust engine.

It combines Microsoft's `vscode-css-languageservice` (standard CSS completion,
hover, diagnostics) with a Rust index of workspace variables and class names,
and uses Babel to map CSS Module imports in JS/TS/TSX.

## Install

```sh
npm install -D karia-lsp
```

## Usage

Launch over stdio from any LSP client:

```sh
karia-lsp --stdio
# or
node node_modules/karia-lsp/dist/server.js --stdio
```

Features: hover/definition/completion for `var(--token)`, class completion and
hover for `styles.foo` in TSX, and reflection of unsaved buffer edits.

For Zed, install the dev extension in
[`editors/zed`](https://github.com/toyamarinyon/karia/tree/main/editors/zed);
it can resolve this package automatically.

## Prototype boundaries

See the [monorepo README](https://github.com/toyamarinyon/karia#readme) — the
index is workspace-wide and does not model the import graph or the DOM
cascade; rename, find-references, and `.d.ts` generation are not implemented.

## License

MIT
