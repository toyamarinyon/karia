# Karia — CSS Modules × CSS variables × LSP

A monorepo experimenting with a setup where editors and agents share the same CSS analysis, using a Vite demo.

```text
Zed / LSP client
  └─ packages/karia-lsp (TypeScript 7 / Node.js)
       ├─ vscode-css-languageservice: standard CSS completion and hover
       ├─ Babel: mapping imports and variable scopes in JS/TS/TSX
       └─ NDJSON worker → packages/karia (Rust / cssparser)
                              ├─ index of variables, classes, and definition sites
                              └─ karia CLI → Agent / CI
```

No dependency on `vscode-langservers-extracted` or a TypeScript Language Service Plugin. The Rust worker is required. No `.d.ts` generation is needed either, though this does not add strict CSS Module types to `tsc`.

## Getting started

Verified environment: Node.js 26.8.2, pnpm 12.4.2, Rust 1.98.1. The Rust version is pinned via `rust-toolchain.toml`. Works on macOS and Linux; GitHub Actions runs build, typecheck, lint, and test on both OSes.

```sh
git clone https://github.com/toyamarinyon/karia.git
cd karia
pnpm install --frozen-lockfile
pnpm run build
pnpm run dev
```

`apps/demo` is the original `hello-vite-css-lsp`. To run on a fixed port:

```sh
pnpm --filter @css-lab/demo run dev --host 127.0.0.1 --port 5175 --strictPort
```

A build is only needed when first setting up the tools or when the implementation changes. Edits to the CSS/TSX you work with are reflected in the LSP without saving or building.

## Try it in an editor

Install the [Zed development extension](editors/zed/README.md) and open the root of this monorepo. `.zed/settings.json` switches the project's CSS server to `karia`, and in TSX it runs alongside the existing TypeScript server.

1. Hover `var(--surface)` in `apps/demo/src/App.module.css` → declared values and definition site in another file.
2. Jump to definition at the same spot → `tokens.css`.
3. Type `var(--` → completion of workspace variables with their values.
4. Type `styles.` in `apps/demo/src/App.tsx` → class completion from `App.module.css`.
5. Add a class or variable to the CSS → reflected in completion without saving.
6. Discard unsaved changes and close the file → falls back to the on-disk definitions.

From a standard LSP client, launch the process below directly. The server's stdout carries only JSON-RPC, so do not start it via `turbo`.

```sh
node packages/karia-lsp/dist/server.js --stdio
```

## Try it from an agent / CI

The built Rust CLI runs without Node.js. From the pnpm workspace you can invoke it with short commands:

```sh
pnpm run karia inspect apps/demo/src --token --surface
pnpm run karia check apps/demo/src --format json
```

`inspect` returns conditions such as declared values, definition sites, and selectors. `check` detects CSS variables with no declaration in the index; it exits with code 1 when diagnostics exist and 2 for invalid arguments. `start`/`end` in the JSON are UTF-8 byte offsets, which the LSP converts to UTF-16 positions.

Unlike `npm run`, pnpm passes arguments to scripts without `--`. Agents that want only JSON output can use `node packages/karia/bin/karia.js …`, which avoids pnpm/Turbo log noise.

The CLI and the LSP share the same Rust index and variable diagnostics. The LSP publishes only `unknown-custom-property`; standard CSS lint is intentionally out of scope and can be covered by running a linter such as Stylelint alongside this server.

## Verification

In local verification we installed the Zed development extension and confirmed on screen that hovering `--surface` shows `#ffffff` and `tokens.css`, and hovering `styles.page` in TSX shows the CSS declaration. In TSX the TypeScript type hover is shown alongside it.

```sh
pnpm run build
pnpm run typecheck
pnpm run lint
pnpm test
pnpm run probe:css
```

- Rust unit tests: Unicode positions, replacement equivalent to unsaved updates, exclusion of comments/strings/`:global`, nesting, escaped identifiers, variable fallbacks.
- Babel unit tests: import mapping, same-named local variables, mid-typing input, string/comment exclusion.
- Inter-process tests: launch the real Rust CLI and stdio LSP to verify completion, hover, definition, edit/close, and file add/delete.
- `probe:css`: preserves the original experiment against the Microsoft library alone.

Turborepo's actual work lives in each package, with the LSP→Rust dependency declared via pnpm workspaces. Native artifacts are architecture-dependent, so Turbo caching is disabled for Rust tasks and Cargo's own incremental build is used instead.

## Prototype boundaries

- CSS variables are indexed across the CSS in the open workspace. Visibility through the import graph and the DOM cascade are unresolved — "a declaration is found" does not mean "it applies to that element".
- Indexing skips `node_modules`, `dist`, `target`, `.git`, `.turbo`, and paths excluded by `.gitignore`. The CLI also honors parent-directory `.gitignore` and `.git/info/exclude`; the LSP only reads `.gitignore` files inside the workspace. Open documents stay indexed even when ignored.
- Hover lists each declaration's value and conditions. Recursive resolution of variable aliases, color swatches, doc comments, rename, and find-references are not implemented.
- CSS Modules covers regular local classes plus `:global`/`:local`. `composes`, ICSS exports, Vite's `localsConvention`, and exact parity with Sass/Less/PostCSS transforms are unsupported.
- In JS/TS/TSX, only default CSS Module imports via relative paths and direct property access are covered. Path aliases, re-exports, destructuring, and dynamic keys are unsupported.
- Diagnostics for unknown classes in TSX and `.d.ts` generation for `tsc` are not yet available. The CLI inspects CSS variables.
- Published diagnostics are limited to `unknown-custom-property`. Standard CSS lint (syntax errors, duplicate declarations, etc.) is delegated to linters such as Stylelint.
- Identifier detection at the CSS cursor position targets ordinary names; hover/completion for references containing CSS escapes has limitations.
- The Rust index re-parses changed CSS. The LSP is an initial implementation that serializes requests to guarantee ordering; performance for large projects has not been optimized yet.

## Sources

- [Microsoft CSS language service](https://github.com/microsoft/vscode-css-languageservice)
- [Microsoft Node LSP library](https://github.com/microsoft/vscode-languageserver-node)
- [cssparser](https://github.com/servo/rust-cssparser)
- [Zed language extensions](https://zed.dev/docs/extensions/languages)

## npm packages

Two packages. `karia` contains the Rust native binary and CLI; `karia-lsp` is the Node LSP that depends on `karia`. Both are published to npm by the release workflow.

```sh
pnpm --dir packages/karia pack --pack-destination /tmp
# You can install the resulting tgz into another project.
```

Native binaries are bundled into the `karia` package as `bin/karia-<os>-<arch>[.exe]` (`linux`/`linux-musl`/`darwin`/`win32` × `x64`/`arm64`). The `bin/karia.js` wrapper picks and launches the binary for your platform from `platform`/`arch` (musl is detected via `ldd` on Linux), and Windows ARM64 falls back to the x64 binary. `binaryPath` in `packages/karia/index.js` performs the same resolution, so the LSP never consults PATH and always uses the binary from its own dependency.

Distribution follows the agent-browser approach. During development, `pnpm run build` copies the binary into `bin/` via `scripts/copy-native.js` after `cargo build`. On release, `.github/workflows/release.yml` builds 7 targets into `bin/`, pnpm-publishes `karia` and `karia-lsp` via npm trusted publishing (OIDC, no token), and creates a GitHub Release tagged `v<version>`. `scripts/postinstall.js` downloads the matching binary from the GitHub Release when none exists in `bin/` (the download is skipped only when the package is marked private). The `version` in `package.json` is synced to `Cargo.toml`/`karia-lsp` by `scripts/sync-version.js`.

After a global install, invoke it as `karia inspect src --token --surface`; installed into a project, use `npx karia inspect src --token --surface`.
