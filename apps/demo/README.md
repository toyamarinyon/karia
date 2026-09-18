# CSS Language Service Lab

A minimal demo of Vite + React + TypeScript + CSS Modules.
It calls Microsoft's `vscode-css-languageservice` directly to verify its behavior without depending on an editor.

```sh
pnpm install
pnpm run dev
```

## Reproduction commands

```sh
pnpm run probe:css
pnpm run typecheck
pnpm run lint
pnpm run build
```

`probe:css` runs `scripts/probe-css.mjs` and saves the details to `probe-results.json`.
It builds CSS in memory with the cursor position marked by `|` and calls the language service APIs.
`src/tokens.css` is read from the actual file. Results are recorded without assuming success.

## Measured results (vscode-css-languageservice 6.3.10)

| Check | Result |
| --- | --- |
| Completion of CSS property names | `display` appears as a candidate |
| Completing same-file variables via `var(--...)` | `--local-accent` appears |
| Parsing another file first with the same service | Variable candidates do not appear |
| Variable completion via `@import "./tokens.css"` | Does not appear |
| Registering variable names as properties via Custom Data, then completing `var(--...)` | Does not appear |
| Declaration-side completion of variable names registered via Custom Data | `--catalog-color` appears |
| Go-to-definition on a same-file variable | Definition position is returned |
| Go-to-definition on a variable in an imported file | null |
| File link for the import | A link to tokens.css is returned |
| Diagnostics for an undefined variable `var(--does-not-exist)` | None |
| Diagnostics for a misspelled CSS property `colro` | Warning present |
| Diagnostics for CSS Modules `:global(body)` | No warning in this example |

Hovering a variable reference showed the description of the CSS `color` property rather than the variable's resolved value in this example.

## What we learned and its limits

This package is a language service library, not a standalone LSP server or editor extension.
**Simply adding it to npm does not change Zed's completion.** This project does not add a custom LSP or Zed configuration.

It can assist with CSS in the same document, but with the standard API usage we tried, it does not automatically search CSS variables across files.
We also passed a file system provider and reference URL resolution, but `@import` link resolution and variable resolution are separate.
Registering `properties` via Custom Data did not substitute for a completion catalog for variable references either.
A host or LSP with additional indexing/completion could change this behavior.

Vite handles CSS Modules itself. To avoid conflating effects with the CSS language service, no plugins or type generation to supplement strict `styles.page` completion were added for this verification.

## Files

- `src/App.tsx`: React component using a CSS Module
- `src/App.module.css`: uses local variables and variables from another file
- `src/tokens.css`: shared variables (loaded via index.css)
- `scripts/probe-css.mjs`: reproduction script for completion, definition, and diagnostics
- `probe-results.json`: actual API responses

Official: https://github.com/microsoft/vscode-css-languageservice
