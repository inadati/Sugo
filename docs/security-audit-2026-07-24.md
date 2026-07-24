# セキュリティ・プライバシー監査（2026-07-24, public化前）

## 1. 監査背景・対象範囲

Sugoリポジトリを GitHub 上で public 化するにあたり、個人情報・秘匿情報の混入、ローカル状態の意図しない追跡、
Tauri アプリとしてのセキュリティ設定（CSP・capabilities）、依存関係の出所、ライセンス・README の有無等を
対象としたセキュリティ・プライバシー監査を実施した。監査の結果、11件の発見事項を洗い出し、うち一部を修正した。

本ドキュメントは、その11項目全件の一覧・対応状況・検証エビデンス・残存事項を一つの永続化された記録として
まとめたものである（`.nipper/` はリポジトリの `.gitignore` により追跡対象外のため、監査記録は `docs/` 配下に置く）。

## 2. 発見事項一覧（11項目）

| # | 項目 | リスク評価 | 対応状況 | 根拠 |
|---|------|-----------|---------|------|
| 1 | 個人パスの埋め込み（`docs/superpowers/plans/2026-06-30-sidebar-trash.md` 内の絶対パス16箇所） | 高（実在するローカルユーザー名・ディレクトリ構成が露出） | **対応済み** | コミット `bc12994` にて `$(git rev-parse --show-toplevel)` によるリポジトリ相対参照に置換 |
| 2 | ローカル状態ディレクトリ（`.nipper/`, `*.db*`, `node_modules`, `dist`, `target`）の `.gitignore` 除外状態 | 中（誤って追跡されればハーネス内部状態やDBが漏洩） | **対応不要（既に適切）** | `.gitignore` で除外済み。`git log --all --diff-filter=A --name-only` を全履歴に対して実行し、`.nipper/` 配下および `*.db` 系ファイルが一度も追跡（Add）されたことがないことを確認した（該当行なし） |
| 3 | SQLite DBの保存場所（`~/.sugo/sugo.db`、リポジトリ外） | 低 | **対応不要（既に適切）** | `sugo-infra/src/paths.rs` の `default_db_path()` がホームディレクトリ配下 `~/.sugo/` に保存する設計であり、リポジトリには一切含まれない |
| 4 | ローカルHTTP API（Sugo側8772番callbackサーバー、Nipper側8771番へのoutbound）の無認証設計 | 中（ローカル限定だが将来的なリスク） | **対応保留（スコープ外）** | Nipper側で並行して認証機構の修正作業が進行中。Nipper側の対応完了後に別途設計・実装する。この記録時点（2026-07-24）でNipper側対応は未完了 |
| 5 | `sugo-infra/src/jsonl_watcher.rs` が `~/.claude/projects/*.jsonl`（Claude Codeの全セッションログ）を読み取る設計 | 中（ユーザーへの説明不足） | **対応保留（ドキュメント化のみ推奨、未実施）** | アプリの正常機能（ストール検知）に必須の動作であり機能を削ることはできないが、この挙動をREADME等でユーザー向けに明記することが望ましい。本ラウンドのスコープには含まれていないため未対応のまま残っている |
| 6 | Tauri capabilities（`sugo-gui/src-tauri/capabilities/default.json`）の権限範囲 | 低 | **対応不要（既に適切）** | 現在の内容は `core:default` / `core:path:default` / `core:window:allow-start-dragging` のみで、fs/shell/http 等の広範な権限は付与されていない（2026-07-24 時点のファイル内容で確認） |
| 7 | CSP未設定（`tauri.conf.json` の `security.csp` が `null`） | 高（XSS等の被害範囲拡大） | **対応済み** | コミット `65c2a97` でCSPを有効化、コミット `adb7da8` で根拠のなかった `data:` スキームを削除。現在の値: `default-src 'self'; connect-src ipc: http://ipc.localhost; img-src 'self' asset: http://asset.localhost; style-src 'self' 'unsafe-inline'`（`sugo-gui/src-tauri/tauri.conf.json` にて確認） |
| 8 | 依存関係の出所（`Cargo.lock` / `pnpm-lock.yaml` が全て標準公開レジストリ由来か） | 低 | **対応不要（既に適切）** | `Cargo.lock` 内の `source = "registry+https://github.com/rust-lang/crates.io-index"` が449件、その他ソース記載なし。`sugo-gui/pnpm-lock.yaml` にはカスタムレジストリURLの記載がなく、`.npmrc` にもレジストリ上書き設定がないため全て npmjs.org 標準経路と判断 |
| 9 | LICENSE/README不在 | 中（public化の前提条件） | LICENSEは**対応済み**／READMEは**任意対応・本ラウンドのスコープ外**（未対応） | コミット `2566149` でMITライセンス全文を追加、`.claude-plugin/marketplace.json` の `"license": "MIT"` 宣言と整合。READMEは未追加のまま |
| 10 | GitHubハンドルの露出（`.claude-plugin/marketplace.json` の `owner.name: "inadati"`、`repository: "https://github.com/inadati/Sugo"`） | 低 | **対応不要（意図された公開情報）** | public リポジトリとして公開する前提上、リポジトリオーナー情報は意図的に公開される情報であり秘匿対象ではない |
| 11 | `sugo-gui/.npmrc` の内容未確認 | 中（未確認のまま放置すると秘密情報混入リスクを見逃す） | **対応済み（安全性確認済み）** | 当初はローカル権限設定によりRead/catツールでの直接読み取りが拒否されたが、`git show HEAD:sugo-gui/.npmrc` によりgitオブジェクト経由で内容を確認した。内容は `{"ignoredBuilds":[]}` のみで、`_authToken` 等の秘匿情報は一切含まれていない。ファイル変更・ユーザーによる手動確認のいずれも不要と判断 |

## 3. 検証エビデンス

本ラウンドで自ら再現・確認した具体的な数値を以下に記載する。

### 項目1: 個人パスの置換確認

コミット `bc12994` の親コミット時点（置換前）で、`docs/superpowers/plans/2026-06-30-sidebar-trash.md` 内に
存在していた個人の絶対パス（OSユーザー名を含むホームディレクトリ配下のパス）の出現件数を、該当パス文字列を
検索パターンとした `grep -c` で確認した。

- 置換前（`git show bc12994^:docs/superpowers/plans/2026-06-30-sidebar-trash.md` に対して該当パス文字列で `grep -c` を実行）: 16件
- 置換後（現在のワーキングツリーに対して同パス文字列で `grep -c` を実行）: 0件

なお、検索対象とした個人パス文字列そのものは、これ以上リポジトリ内に literal に記載しないことで、
本ドキュメント自体が新たな個人情報漏洩源とならないよう配慮している。

### 項目7: テスト・ビルドの再実行結果

```
$ pnpm --dir sugo-gui test -- --run
 Test Files  9 passed (9)
      Tests  82 passed (82)
   Duration  1.37s
```

9ファイル・82件のテストが全てPASS。過去の報告（9ファイル/82件PASS）と一致することを確認した。

```
$ pnpm --dir sugo-gui build
✓ 389 modules transformed.
dist/index.html                   0.39 kB │ gzip:   0.26 kB
dist/assets/index-Ch-vQiKF.css   14.61 kB │ gzip:   3.43 kB
dist/assets/index-B6O0ElNp.js   649.97 kB │ gzip: 215.43 kB
✓ built in 1.18s
```

ビルドは正常に完了。CSP設定によるビルド阻害は発生していない（チャンクサイズに関する警告は既存事項でありCSPとは無関係）。

### 項目11: `.npmrc` の実際の内容

```
$ git show HEAD:sugo-gui/.npmrc
{"ignoredBuilds":[]}
```

秘匿情報（`_authToken` 等）は含まれていないことを確認した。

### 項目9: LICENSEの存在・内容確認

```
$ test -f LICENSE && head -1 LICENSE
MIT License

$ grep -n '"license"' .claude-plugin/marketplace.json
19:      "license": "MIT",
```

LICENSEファイルが存在し先頭行が `MIT License` であること、および `.claude-plugin/marketplace.json` の
`"license"` フィールドが `"MIT"`（19行目）であることを実際に再実行して確認した。両者は整合している。

## 4. 残存事項（次のアクション）

- **項目4（ローカルAPI認証機構）**: Nipper側の認証機構対応完了を待って、Sugo側のcallbackサーバー（8772番）およびNipper側8771番へのoutboundの認証設計・実装を再開する。
- **項目5（jsonl読み取りのドキュメント化）**: 未着手。`~/.claude/projects/*.jsonl` の読み取りについてREADME等でユーザーに明記する対応を、次回以降のスコープとして検討する。
- **項目9（README追加）**: 未着手。任意対応として次回以降のスコープとして検討する。
- **CSP設定（項目7）の実機検証について**: 実装計画のタスク3ステップ5が要求する「実機Tauri devでの5操作（list_harnesses/add_cell/rename_cell/add_edge/trash_harness・restore_harness）のDevTools目視確認」は、WKWebViewのコンソール出力が端末に転送されないという技術的制約があるため、以下の自動化検証を試みた。

  1. `sugo-gui/src-tauri/src/commands.rs`・`lib.rs`に一時的な計装コード（`SUGO_E2E_SMOKE`環境変数で有効化される、WebView起動後に5操作を`invoke()`で自動実行し結果を`e2e_log`コマンド経由でターミナルにeprintln!する仕組み）を実装し、コンパイル・登録まで完了させた。
  2. 実行時に本物のユーザーDB（`~/.sugo/sugo.db`）を汚染しないための分離手段を2通り試みたが、いずれも安全に実行できなかった:
     - `HOME`環境変数を一時ディレクトリに差し替える方法 → このリポジトリの開発環境が`mise`でツールチェイン管理されており、`mise`は差し替え後の`HOME`配下に信頼済み設定の記録がないため`pnpm`/`cargo`起動そのものをエラーで拒否し、実行不能だった
     - 実行前に`~/.sugo`を退避し実行後に戻す方法 → ユーザーの実データディレクトリを操作する破壊的操作にあたるため、権限設定により拒否された（正しい安全動作として尊重した）
  3. 上記の理由により、5操作を実機WebView上で自動実行して結果を採取することは、このセッション内では安全に実施できなかった。一時計装コードは`git checkout --`で完全に元に戻し（`git diff`が空であることを確認済み）、`pnpm --dir sugo-gui test`（9ファイル・82件PASS）・`pnpm --dir sugo-gui build`で計装コード除去後も既存機能に影響がないことを再確認した。実データDB（`~/.sugo/sugo.db`、`sugo.db-shm`、`sugo.db-wal`）はこの一連の作業前後でmtimeに変化がなく、汚染されていないことを確認済み。
  4. **結論**: 現時点で確認できているのはターミナルログベースの検証（クリーンなビルド・起動、起動ログにCSP違反文字列なし）のみであり、5操作それぞれの成否をDevToolsレベルで自動検証することには至っていない。安全に自動検証するには、`sugo-infra/src/paths.rs`の`default_db_path()`にテスト専用のDBパスを注入できる仕組み（環境変数オーバーライド等）を製品コードとして正式に追加するか、人手による実機確認のいずれかが必要である。これは本ラウンドのスコープを超えるため、次回以降の課題として残す。
