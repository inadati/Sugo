# CLAUDE.md

このファイルは Claude Code が Sugo リポジトリで作業する際のガイダンスを提供する。

## Sugo とは

Sugo（スーゴ）は、AIハーネスを「双六盤を組み立てるように」作る **Tauri製デスクトップアプリ**。
ハーネスは「マス（cell）」を節点とする有向グラフ（分岐・ループ・ガード・始端/終端を持つ FSM）。各マスに登録プロンプトが紐づく。

詳細仕様は `SPEC.md` を参照。

## アーキテクチャ要点

- **Sugo**: ハーネスの作成・編集・永続化・進行制御
- **Nipper**: Claude Code を PTY 起動するチャットUIアプリ。ターンを駆動
- **Claude Code**: マスのプロンプトに従い実作業・マス著作・整合修正を行う頭脳
- 参照実装は Nipper の `chot-harness`（システム駆動で確実に進行を回す）

## 連携の仕組み

- Sugo は PTY を持たない。`sugo_advance` MCP 呼び出し ＋ プロジェクト jsonl 監視で進行を駆動
- Nipper のローカル HTTP inject API 経由で指示を注入（ローカル限定・トークン不要）
- ルーティング鍵は **プロジェクトパス**（session-id は resume でフォークするため不採用）
- パス曖昧性は jsonl の `cwd` フィールドで照合

## データ方針

- **SQLite DB を持つ**。`.sugo/` 共有ファイルは持たず、調整情報はすべて DB に集約。DB が排他の権威
- **盤面定義は不変 JSON**（`board_versions`）、**実行状態/レジストリ/イベント/outbox は正規化テーブル**（ハイブリッド）
- run は board_version にピン留め

## MCP / スキル方針

- MCP ツール: マップ全体＋現在地取得、`sugo_edit_cell` / `sugo_validate_harness` / `sugo_update_harness` / `sugo_start` / `sugo_status` / `sugo_advance` 等
- スキルは単一リポジトリ内 `Sugo/claude-plugins/<plugin>/skills/<skill>/SKILL.md` に同梱（Nipper の `claude-plugins/chot-harness/` と同構成）
- マスへのプロンプト登録時は **①ハーネス全体の整合性を踏まえる ②後戻りしにくい判断はユーザーに確認する** をスキルで規律化
- ドラフトを持つハーネス発見時は先送りせず全ハーネスを整合修正・充填→`draft → active` 昇格→ユーザー報告。ドラフト残存で `sugo_start` はハードエラー

## git 運用

- Sugo は **独立した git リポジトリ**（親 Asweed の `.gitignore` で `sandbox/*` が除外済み）
- コミットは必ず Sugo 内で行う
