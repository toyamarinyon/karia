# CSS Language Service Lab

Vite + React + TypeScript + CSS Modules の最小デモ。
Microsoft の `vscode-css-languageservice` を直接呼び、エディタに依存せず挙動を検証します。

```sh
npm install
npm run dev
```

## 再現コマンド

```sh
npm run probe:css
npm run typecheck
npm run lint
npm run build
```

`probe:css` は `scripts/probe-css.mjs` を実行し、詳細を `probe-results.json` に保存します。
カーソル位置を `|` で指定したCSSをメモリ上に作り、言語サービスのAPIを呼びます。
`src/tokens.css` は実際のファイルを読み込みます。結果は成功を仮定せず記録します。

## 実測結果（vscode-css-languageservice 6.3.10）

| 検証 | 結果 |
| --- | --- |
| CSSプロパティ名の補完 | `display` が候補に出る |
| 同一ファイルの変数を `var(--...)` で補完 | `--local-accent` が出る |
| 別ファイルを同じサービスで先にparse | 変数候補に出ない |
| `@import "./tokens.css"` 経由の変数補完 | 出ない |
| Custom Dataで変数名をpropertiesへ登録し `var(--...)` を補完 | 出ない |
| Custom Dataで登録した変数名の宣言側補完 | `--catalog-color` が出る |
| 同一ファイルの変数の定義ジャンプ | 定義位置が返る |
| import先の変数の定義ジャンプ | null |
| importのファイルリンク | tokens.cssへのリンクが返る |
| 未定義変数 `var(--does-not-exist)` の診断 | なし |
| CSSプロパティの誤字 `colro` の診断 | 警告あり |
| CSS Modulesの `:global(body)` の診断 | 今回の例では警告なし |

変数参照上のhoverは、今回の例では変数の解決値ではなくCSSの `color` プロパティの説明でした。

## 分かることと限界

このパッケージは言語サービスのライブラリで、単体のLSPサーバーやエディタ拡張ではありません。
**npmに入れるだけでZedの補完が変わるわけではありません。** このプロジェクトには独自LSPやZed設定を追加していません。

同一ドキュメントのCSS支援はできますが、今回試した標準APIの使い方では、別ファイルのCSS変数を自動で横断検索しません。
ファイルシステムプロバイダーと参照URL解決も渡していますが、`@import` のリンク解決と変数の解決は別です。
Custom Dataの `properties` 登録も、変数参照の補完カタログの代わりにはなりませんでした。
追加の索引・補完処理を持つホストやLSPなら挙動は変わり得ます。

CSS Modules自体はViteが処理します。`styles.page` の厳密な補完を補うプラグインや型生成は、CSS言語サービスとの効果の混同を避けるため、この検証には追加していません。

## ファイル

- `src/App.tsx`: CSS Moduleを利用するReactコンポーネント
- `src/App.module.css`: ローカル変数と別ファイルの変数を利用
- `src/tokens.css`: 共通変数（index.css経由で読み込み）
- `scripts/probe-css.mjs`: 補完・定義・診断の再現スクリプト
- `probe-results.json`: 実際のAPI応答

公式: https://github.com/microsoft/vscode-css-languageservice
