# Karia for VS Code

Development adapter that runs the `karia-lsp` language server in VS Code.
Core features live in `packages/karia-lsp`; this extension only resolves and
launches the server.

## Server resolution

1. `karia.serverPath` setting (absolute path to `server.js`)
2. `karia-lsp` installed in the workspace (`pnpm add -D karia-lsp`)

The server is launched as `node server.js --stdio`.

## Development

```sh
pnpm install
pnpm run build
```

Then press F5 in VS Code with this folder open to launch an Extension
Development Host.
