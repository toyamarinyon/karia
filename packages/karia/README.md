# karia

Query CSS custom properties and CSS Modules using a Rust analysis engine.

`karia` bundles prebuilt native binaries for Linux (GNU/musl), macOS, and
Windows (x64/arm64). If your platform's binary is missing from the package,
the postinstall script downloads it from the matching GitHub Release.

## Install

```sh
npm install -g karia-css
# or in a project
npm install -D karia-css
```

## Usage

```sh
# Inspect a custom property: declared values, definition sites, selectors
karia inspect src --token --surface

# Report CSS variables that have no declaration in the index
karia check src --format json
```

`check` exits with code 1 when diagnostics exist and 2 for invalid arguments.
`start`/`end` in the JSON output are UTF-8 byte offsets.

From Node.js, `binaryPath` in `karia-css` resolves the bundled native binary for
the current platform without consulting PATH.

## Prototype boundaries

- The index covers CSS files under the scanned directory. Visibility through
  the import graph and the DOM cascade is unresolved.
- `check` reports undeclared variable references; it is not a full CSS lint.

See the [monorepo README](https://github.com/toyamarinyon/karia#readme) for
the full architecture and limitations.

## License

MIT
