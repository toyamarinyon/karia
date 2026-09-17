# Karia — CSS Modules × CSS variables × LSP

Viteデモを使って、エディタとAgentが同じCSS解析を利用する構成を試すモノレポです。

```text
Zed / LSP client
  └─ packages/css-lsp (TypeScript 7 / Node.js)
       ├─ vscode-css-languageservice: 標準CSSの補完・hover・診断
       ├─ Babel: JS/TS/TSXのimportと変数スコープの対応付け
       └─ NDJSON worker → packages/css-core (Rust / cssparser)
                              ├─ 変数・クラス・定義位置の索引
                              └─ karia CLI → Agent / CI
```

`vscode-langservers-extracted`やTypeScript Language Service Pluginには依存しません。Rust workerは必須です。`.d.ts`生成も不要ですが、`tsc`にCSS Moduleの厳密な型を追加するものではありません。

## 起動

検証環境: Node.js 26.8.2、npm 11.19.1、Rust 1.98.1。Rustのバージョンは`rust-toolchain.toml`で固定しています。

```sh
git clone https://github.com/toyamarinyon/karia.git
cd karia
npm ci
npm run build
npm run dev
```

`apps/demo`が元の`hello-vite-css-lsp`です。ポート固定で起動する場合:

```sh
npm run dev --workspace=@css-lab/demo -- --host 127.0.0.1 --port 5175 --strictPort
```

ビルドが必要なのはツール自身の初回導入・実装変更時です。利用するCSS/TSXの編集時は、保存やbuildをせずLSPへ反映します。

## エディタで試す

[Zed開発用拡張](editors/zed/README.md)をインストールし、このモノレポのルートを開いてください。`.zed/settings.json`はプロジェクト内のCSSサーバーを`css-lab`に切り替え、TSXでは既存のTypeScriptサーバーと併用します。

1. `apps/demo/src/App.module.css`で`var(--surface)`をhover → 別ファイルの宣言値と定義元。
2. 同じ場所で定義ジャンプ → `tokens.css`。
3. `var(--`と入力 → ワークスペースの変数と値の補完。
4. `apps/demo/src/App.tsx`で`styles.`と入力 → `App.module.css`のクラス補完。
5. CSSにクラスや変数を追加 → 未保存のまま補完へ反映。
6. 未保存の変更を破棄してファイルを閉じる → ディスク上の定義に戻る。

標準LSPクライアントからは、以下のプロセスを直接起動します。LSPのstdoutにはJSON-RPCだけを流すため、`turbo`経由でサーバーを起動しないでください。

```sh
node packages/css-lsp/dist/server.js --stdio
```

## Agent / CIから試す

ビルド済みRust CLIはNode.jsなしで動きます。npm workspaceからは次の短いコマンドで呼び出せます。

```sh
npm run karia -- inspect apps/demo/src --token --surface
npm run karia -- check apps/demo/src --format json
```

`inspect`は宣言値・定義元・セレクターなどの条件を返します。`check`は索引内に宣言が見つからないCSS変数を検出し、診断があれば終了コード1、引数不正は2です。JSONの`start`/`end`はUTF-8バイト位置です。LSP側ではUTF-16位置へ変換します。

`npm run karia -- …`の`--`は引数をCLIへ渡すために必要です。JSONだけを受け取りたいAgentからは、npm/Turboのログが混ざらない`npx --no-install karia …`を使えます。

CLIとLSPはRustの同じ索引・変数診断を使います。標準CSS構文診断はNode側のMicrosoftライブラリが担当するため、CLIの`check`はCSS全体のlintではありません。

## 検証

開発環境での検証では、Zed開発用拡張をインストールし、`--surface`のhoverに`#ffffff`と`tokens.css`、TSXの`styles.page`のhoverにCSS宣言が表示されることを画面上でも確認しました。TSXではTypeScript側の型hoverも併記されます。

```sh
npm run build
npm run typecheck
npm run lint
npm test
npm run probe:css
```

- Rust単体テスト: Unicode位置、未保存更新相当の置換、コメント・文字列・`:global`除外、ネスト、エスケープ識別子、変数fallback。
- Babel単体テスト: importの対応付け、同名ローカル変数、途中入力、文字列・コメント除外。
- プロセス間テスト: 実際のRust CLIとstdio LSPを起動して補完・hover・定義・編集/close・ファイル追加/削除を検証。
- `probe:css`: Microsoftライブラリ単体の元の実験を維持。

Turborepoの実処理は各パッケージに置き、LSP→Rustの依存をnpm workspaceに宣言しています。ネイティブ成果物はアーキテクチャ依存のため、RustタスクのTurboキャッシュを無効にし、Cargo自身の増分ビルドを使います。

## プロトタイプの境界

- CSS変数は開いたワークスペースのCSSを索引化します。importグラフによる可視性やDOMのカスケードは未解決です。「宣言が見つかる」は「その要素に適用される」と同義ではありません。
- hoverは各宣言の値と条件を列挙します。変数エイリアスの再帰解決、色スウォッチ、説明コメント、rename、参照検索は未実装です。
- CSS Modulesは通常のローカルクラスと`:global`/`:local`を対象とします。`composes`、ICSS export、Viteの`localsConvention`、Sass/Less/PostCSS変換との完全一致は未対応です。
- JS/TS/TSXでは相対パスのdefault CSS Module importと直接のプロパティアクセスが対象です。パスエイリアス、再export、分割代入、動的キーは未対応です。
- TSXで不明なクラスの診断や`tsc`用の型定義生成はまだありません。CLIが検査するのはCSS変数です。
- CSSカーソル位置の識別子判定は通常の名前を対象とし、CSSエスケープを含む参照のhover/補完には制限があります。
- Rust索引は変更されたCSSを再解析します。LSPはリクエストを直列化して順序を保証する初期実装で、大規模プロジェクトの性能最適化はまだ行っていません。

## ソース

- [Microsoft CSS language service](https://github.com/microsoft/vscode-css-languageservice)
- [Microsoft Node LSP library](https://github.com/microsoft/vscode-languageserver-node)
- [cssparser](https://github.com/servo/rust-cssparser)
- [Zed language extensions](https://zed.dev/docs/extensions/languages)

## npmパッケージ

公開名・コマンド名は`karia`です。現在は未公開のローカルパッケージで、`private: true`にしてあります。

```sh
npm pack --workspace=karia --pack-destination /tmp
# 作成されたtgzを別のプロジェクトへインストールして利用できます。
```

現在のパッケージは検証環境のmacOS arm64バイナリを同梱し、`os`/`cpu`で対象を明示しています。一般公開前にはOS・CPU別の配布を整備します。グローバルインストール後の呼び出しは`karia inspect src --token --surface`、プロジェクトへのインストールでは`npx karia inspect src --token --surface`です。
