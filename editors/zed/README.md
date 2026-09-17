# Karia LSP for Zed (開発用)

このディレクトリは、`karia` ワークスペース内の Karia LSP を Zed から起動する最小の dev extension です。CSS、TSX、TypeScript、JavaScript に `css-lab` language server を登録します。文法（Tree-sitter grammar）は追加せず、Zed 標準の言語サポートをそのまま使います。

## インストール

1. ワークスペースの依存関係をインストールし、LSP をビルドします。

   ```sh
   npm install
   npm run build -- --filter=@css-lab/lsp...
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
      "language_servers": ["css-lab"]
    }
  }
}
```

TypeScript 系の既存サーバーは維持してください。TSX / TypeScript / JavaScript でも CSS Lab を有効にする場合は、各言語の `language_servers` の先頭に `css-lab` を追加し、残りに `...` を指定します。
