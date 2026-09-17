# Karia CSS tooling lab

- `apps/demo`: Vite + React + CSS Modules reference app.
- `packages/css-core`: Rust parsing/index library, standalone CLI, NDJSON worker.
- `packages/css-lsp`: Node/TypeScript LSP wrapping Microsoft vscode-css-languageservice and the Rust worker.
- `editors/zed`: thin development adapter; core features must stay editor-independent.

Use npm workspaces and Turborepo. Root scripts only delegate to `turbo run`; actual commands live in packages. Declare workspace dependencies to order builds. Do not add typescript-plugin-css-modules or vscode-langservers-extracted. Do not rely on TypeScript compiler internals.

Preserve unsaved document overlays. Rust offsets are UTF-8 bytes; LSP positions are UTF-16. Never claim a custom property has one computed value when multiple CSS contexts may apply.

Run `npm run build`, `npm run typecheck`, `npm run lint`, and `npm test`. Protocol tests must cover completion, hover, definition and unsaved updates. Document prototype limitations rather than silently approximating full CSS cascade or TypeScript semantics.
