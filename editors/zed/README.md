# Karia LSP for Zed

`karia-lsp`（npm パッケージの CSS language server）を Zed から起動する拡張です。CSS、TSX、TypeScript、JavaScript に `karia` language server を登録します。文法（Tree-sitter grammar）は追加せず、Zed 標準の言語サポートをそのまま使います。

## サーバーの解決順

拡張は次の順で LSP を探します。

1. `lsp.karia.binary` 設定（`path` と任意の `arguments`）。指定すればそのコマンドをそのまま使います。
2. 開いている worktree の `node_modules/karia-lsp`。プロジェクトが lockfile で固定したバージョンを、LSP・`npx karia`・CI で共通にできます。
3. 拡張専用の work dir へ `npm_install_package("karia-lsp")`。最新バージョンを Zed 同梱の Node でインストールします。

`karia-lsp` は `karia` パッケージに依存し、LSP はその中のネイティブバイナリを `binaryPath` 経由で起動します（PATH は見ません）。node は `zed::node_binary_path()` → `which("node")` → `CSS_LAB_NODE` の順で解決します。

## プロジェクトごとの起動指定

settings.json または `.zed/settings.json` で上書きできます。

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

開発中はこの設定でモノレポの `packages/karia-lsp/dist/server.js` を指すと、npm install 経路を使わずに済みます。

## インストール

1. ワークスペースの依存関係をインストールし、LSP をビルドします。

   ```sh
   npm install
   npm run build -- --filter=karia-lsp...
   ```

2. Zed で Command Palette (`cmd-shift-p`) を開き、`zed: install dev extension` を実行します。
3. この `editors/zed` ディレクトリを選択します。
4. `karia` のルートを Zed で開き、CSS ファイルを開いて LSP を起動します。

Zed の現行 extension API は `wasm32-wasip2` を使います。`rustup` を使っている場合は Zed が target を用意します。手動で検証する場合は次を実行します。

```sh
rustup target add wasm32-wasip2
cargo check --target wasm32-wasip2
```

Node の解決は、まず Zed worktree の環境にある `CSS_LAB_NODE`、次に `node` の PATH を使います。`CSS_LAB_NODE` は実行ファイルの絶対パスを指定できます。

## プロジェクト限定の CSS サーバー設定

既定の CSS language server と診断が重ならないよう、必要な場合だけ `karia/.zed/settings.json` に次を追加します。これはプロジェクト設定であり、グローバル Zed 設定は変更しません。

```json
{
  "languages": {
    "CSS": {
      "language_servers": ["karia"]
    }
  }
}
```

TypeScript 系の既存サーバーは維持してください。TSX / TypeScript / JavaScript でも Karia を有効にする場合は、各言語の `language_servers` の先頭に `karia` を追加し、残りに `...` を指定します。

## 既知の制限

- `npm_install_package` / 設定経路は `cargo check --target wasm32-wasip2` でコンパイル確認済みですが、Zed 実機での動作確認は未実施です。
- worktree の `node_modules` 判定は `node_modules/karia-lsp/package.json` の存在で行います。
