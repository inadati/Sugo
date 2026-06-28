# Sugo 開発ロードマップ

Sugo は複数の独立サブシステムを含むため、段階的（縦切り）に開発する。
各 Phase は独立した縦切りで、完了条件（Exit）を満たしたら次へ進む。本書は進捗に応じて更新する。

仕様の全体は `SPEC.md` を参照。

---

## Phase 一覧

| Phase | 名称 | 依存 | 状態 |
|---|---|---|---|
| P1 | コア基盤（DB＋最小MCP） | なし | **完了** (2026-06-28) |
| P3 | GUI（盤面の見える化＋編集） | P1 | **完了** (2026-06-28) |
| P2-core | 進行エンジン（Nipper連携なし） | P1 | **完了** (2026-06-28) |
| P4 | ドラフト確定＆整合運用 | P1, P3 | 未着手 |
| P5 | プラグイン/スキル | P1〜P4 | 未着手 |
| P2-nipper | Nipper inject API 接続（手動） | P2-core, Nipper #179 | 未着手 |

> **順序について**: P2 は Nipper #179（inject API）への依存を切り離し、コア実装（P2-core）を先行させる。
> Nipper 接続（P2-nipper）は全フェーズが揃った後、人間が手動で最後に繋ぐ。

---

## P1: コア基盤（DB＋最小MCP）【完了】

すべての土台となるデータモデルとその操作APIを作る。

**成果物**
- SQLite スキーマ（`harnesses` / `board_versions`）
- 最小 MCP ツール: `sugo_create_harness` / `sugo_status` / `sugo_edit_cell` / `sugo_validate_harness`
- ヘキサゴナルアーキテクチャ（sugo-core / sugo-infra / sugo-mcp）

**完了条件（Exit）** ✅
- ハーネスを DB に作成・取得・検証できる
- 盤面定義が不変な board_version としてバージョニングされる
- 編集は既存 board_version を書き換えず新バージョンを生成する（楽観的ロック付き）

---

## P3: GUI（盤面の見える化＋編集）【完了】

盤面を可視化し、ユーザーが手動でマスを追加できる Tauri フロントエンド。

**成果物**
- Tauri 盤面エディタ（sugo-gui クレート: Vue 3 + Vue Flow）
- マス追加 UI（追加したマスは `status: draft`）
- `sugo_status` の差分表示（追加されたマスをエージェントと素早く認識共有）

**完了条件（Exit）** ✅
- GUI でマスを追加すると draft 登録され、差分がエージェント側から見える

**依存**: P1

---

## P2-core: 進行エンジン（Nipper連携なし）

ハーネスを自動進行させる心臓部。Nipper への注入以外のコアロジックを実装する。

**成果物**
- `runs` テーブル（run を board_version にピン留め）
- `sugo_start`: 実行開始（run 作成・初期マス決定）
- `sugo_advance`: 進行（現在マスのプロンプトを返し、次マスへ遷移）
- jsonl 監視＋ストール検知（`~/.claude/projects/<sanitized>/<session-id>.jsonl`、`cwd` 照合）

**完了条件（Exit）** ✅
- 1 本のハーネスをマス→マスへ自動進行できる（ループ・分岐・ガードを含む）
- Nipper 注入なしでもツール呼び出しで動作検証できる

**依存**: P1

**スコープ外**
- Nipper inject API 呼び出し → P2-nipper

---

## P4: ドラフト確定＆整合運用

ドラフトの確定と全ハーネス整合修正の運用フロー。

**成果物**
- `sugo_update_harness`: 整合性修正の反映
- ドラフト一括解決フロー: ドラフト保持ハーネス発見時に先送りせず全ハーネスを整合修正・充填→`draft → active` 昇格→ユーザー報告
- `sugo_start` のドラフト残存ハードエラー（実行ガード）

**完了条件（Exit）**
- draft→active 昇格、全ハーネス整合修正、実行ガードが効く

**依存**: P1, P3

---

## P5: プラグイン/スキル

エージェントの著作・整合修正の行動規律をスキル化する。

**成果物**
- `Sugo/claude-plugins/<plugin>/skills/<skill>/SKILL.md`（Nipper の `claude-plugins/chot-harness/` と同構成）
- スキルに明記する規律:
  1. マスへのプロンプト登録時、ハーネス全体の整合性を踏まえる
  2. 後戻りしにくい設計判断は勝手に確定せずユーザーに意思決定を求める
  3. ドラフト一括解決の手順

**完了条件（Exit）**
- エージェントがスキルに従いマス著作・整合修正を行う

**依存**: P1〜P4

---

## P2-nipper: Nipper inject API 接続（手動・最終工程）

P2-core で実装した進行エンジンを Nipper のローカル inject API に接続する。
全フェーズが揃った後、人間が最終工程として手動で実施する。

**成果物**
- `sugo_advance` から Nipper inject API（ローカル HTTP）を呼び出す実装
- ルーティング鍵（プロジェクトパス）による inject 先特定
- jsonl の `cwd` フィールドで照合するセッション同定

**完了条件（Exit）**
- Nipper 上で `sugo_advance` を呼ぶと次マスのプロンプトが Claude Code に自動注入される

**依存**: P2-core, Nipper #179（外部）
