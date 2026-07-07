---
name: sugo-cell-author
description: |
  Use this skill when writing, editing, or adding a cell prompt in a Sugo harness.
  Apply these rules whenever you call sugo_edit_cell, or sugo_update_harness with
  cell_changes and/or cell_add (adding a brand-new cell to an existing harness).
version: 0.2.0
tools: []
---

# sugo-cell-author

セルにプロンプトを書く・変更する・新規追加するときの行動規律。

## 手順

### 1. 全体把握を先に行う

`sugo_status()` でハーネス全体の構造を確認してからプロンプトを起草する。
対象セル（`cell_add` の場合は追加予定の新規セル）の役割・前後のセルとの文脈・エッジの接続を理解する。

### 2. 整合性を踏まえてプロンプトを起草する

- 他のセルとの役割分担が重複・矛盾しないか確認する
- プロンプトは「このセルだけを見る者」ではなく「ハーネス全体を知るエージェント」として書く
- `cell_add` で新規セルを著作する場合も同様。新規セルは既存フローに割り込む形になるため、どのセルから遷移し・どのセルへ遷移するかを含めて役割を設計する

### 3. 後戻りしにくい判断はユーザーに確認する

以下に該当する場合は実施前に必ずユーザーに確認を求める:

- エッジの追加・削除（フロー構造の変更）
- セルの分割・統合（セル数の増減）。`sugo_update_harness` の `cell_add` で既存ハーネスに新規セルを追加する操作もこれに含まれる（新規セルの `prompt`/`name`/`status`/`terminal` を丸ごと著作するため、既存セルへの部分的なプロンプト修正より判断の重みが大きい）
- セルの役割の大幅な変更（プロンプトの方向性が変わる場合）
- `start` セルや `terminal` セルの変更
- 複数セルにまたがる一括変更

### 4. request_memo の残留に注意する

対象セルは、人間が残した `request_memo`（AIへのプロンプト改訂依頼メモ、非空なら `status: Draft`）を持っている可能性がある。
プロンプトを編集する前に、`sugo_get_cell(harness_id, cell_id)` で `memo` フィールドを確認すること。

- `memo` が非空のセルのプロンプトを編集する場合、`sugo_edit_cell` だけを使うと `memo` が古いまま残留する（`sugo_edit_cell` には memo をクリアする手段がない）。
  この場合は `sugo_update_harness` の `cell_changes` に `prompt` と `memo: ""` を同時に渡し、明示的にメモをクリアすること。
- ハーネスが `has_draft: true` の場合は、先に `sugo:sugo-draft-resolve` スキルに従って draft セルを解決してから作業する。

### 5. 適用するツール

| 変更内容 | 使用ツール |
|---------|-----------|
| 1セルのみプロンプト変更（`request_memo` なし） | `sugo_edit_cell` |
| 1セルのプロンプト変更 + `request_memo` のクリア | `sugo_update_harness`（`cell_changes` に `prompt` + `memo: ""`） |
| 複数セル・エッジを同時変更 | `sugo_update_harness` |
| 既存ハーネスへの新規セル追加（`id`/`name`/`prompt`/`status`/`terminal` を著作） | `sugo_update_harness`（`cell_add`）。手順3のユーザー確認を経てから実施し、同一呼び出しの `edge_add` で新規セルへ接続してよい |
| セルの現行プロンプト・memoの確認 | `sugo_get_cell(harness_id, cell_id)` |
