# Karia LSP for Zed

A Zed extension that launches `karia-lsp` (the CSS language server from the npm package). It registers the `karia` language server for CSS, TSX, TypeScript, and JavaScript. It does not add a grammar (Tree-sitter grammar); Zed's built-in language support is used as-is.

## Server resolution order

The extension looks for the LSP in the following order:

1. The `lsp.karia.binary` setting (`path` and optional `arguments`). If specified, that command is used as-is.
2. `node_modules/karia-lsp` in the open worktree. This lets the LSP, `npx karia`, and CI share the version pinned by the project's lockfile.
3. `npm_install_package("karia-lsp")` into the extension's own work dir. Installs the latest version using the Node bundled with Zed.

`karia-lsp` depends on the `karia` package, and the LSP launches the native binary inside it via `binaryPath` (PATH is not consulted). Node is resolved in the order: `zed::node_binary_path()` → `which("node")` → `CSS_LAB_NODE`.

## Per-project launch configuration

You can override it in settings.json or `.zed/settings.json`:

```json
{
  "lsp": {
    "karia": {
      "binary": {
        "path": "/path/to/node",
        "arguments": ["/path/to/karia/packages/karia-lsp/dist/server.js", "--stdio"]
      }
    }
  }
}
```

During development, pointing this at the monorepo's `packages/karia-lsp/dist/server.js` avoids the package install path.

## Installation

1. Install the workspace dependencies and build the LSP.

   ```sh
   pnpm install
   pnpm run build --filter=karia-lsp...
   ```

2. In Zed, open the Command Palette (`cmd-shift-p`) and run `zed: install dev extension`.
3. Select this `editors/zed` directory.
4. Open the `karia` root in Zed and open a CSS file to start the LSP.

Zed's current extension API uses `wasm32-wasip2`. If you use `rustup`, Zed will provide the target. To verify manually, run:

```sh
rustup target add wasm32-wasip2
cargo check --target wasm32-wasip2
```

Node resolution first uses `CSS_LAB_NODE` from the Zed worktree environment, then `node` on PATH. `CSS_LAB_NODE` can be an absolute path to the executable.

## Project-local CSS server configuration

To avoid overlapping with the default CSS language server's diagnostics, add the following to `karia/.zed/settings.json` only when needed. This is a project setting and does not change your global Zed settings.

```json
{
  "languages": {
    "CSS": {
      "language_servers": ["karia"]
    }
  }
}
```

Keep the existing TypeScript servers. To enable Karia in TSX / TypeScript / JavaScript as well, add `karia` to the front of each language's `language_servers` and put `...` for the rest.

## Known limitations

- The `npm_install_package` / settings paths compile-check with `cargo check --target wasm32-wasip2`, but have not been verified on a real Zed installation.
- The worktree `node_modules` check is based on the presence of `node_modules/karia-lsp/package.json`.
