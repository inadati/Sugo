---
name: sugo-draft-resolve
description: |
  Use this skill immediately when you discover a harness with has_draft: true.
  Do not defer. Resolve all draft cells before proceeding with other work.
version: 0.1.0
tools: []
---

# sugo-draft-resolve

`has_draft: true` のハーネスを発見したとき、先送りせず一括解決するフロー。

## 発動条件

`sugo_status()` 等の結果で `has_draft: true` のハーネスを発見したとき。
**先送り禁止。他の作業より先にこのスキルを実行する。**

## 手順

### 1. 全ハーネスを走査する

`sugo_status()` で全ハーネス一覧を取得し、`has_draft: true` のハーネスをすべて列挙する。
複数ある場合は1つずつ順番に解決する。

### 2. ドラフトセルのプロンプトを起草する

対象ハーネスの `sugo_status(harness_id)` で `draft_diff` を確認する。`draft_diff` の各エントリは
`{cell_id, name, memo}` を持つ。

プロンプト起草のルール:
- ハーネス全体の構造・他のセルとの整合性を踏まえて起草する
- **`memo` が空文字の場合**（新規追加されたセル）: プロンプトは必ず充填する（空のままにしない）。
  このセルが担う役割・次のセルへの接続・期待する成果物を明確に書く
- **`memo` が非空の場合**（既存セルへの改訂依頼）: 現在の `prompt`（`sugo_status` はセルの
  プロンプトを一切返さないため、常に `sugo_edit_cell` で対象セルの現行プロンプトを
  取得する）と `memo` の内容を踏まえてプロンプトを改訂する。memo の要望を
  そのまま転記するのではなく、既存プロンプトの文脈に合わせて自然に反映する

### 3. `sugo_update_harness` で一括適用する

draft セルすべてに以下を設定して1回のツール呼び出しで送信する:
- `prompt`: 充填・改訂したプロンプト
- `status`: `"active"`
- `memo`: `""`（メモ由来の改訂だった場合、対応済みとして必ずクリアする）

`expected_lock_version` は `sugo_status(harness_id)` で取得した値を使う。

```json
{
  "harness_id": "<id>",
  "expected_lock_version": <lock_version>,
  "cell_changes": [
    { "cell_id": "<draft_cell_id>", "prompt": "<充填/改訂プロンプト>", "status": "active", "memo": "" }
  ]
}
```

### 4. 解決を確認する

`sugo_status(harness_id)` を呼び、`has_draft: false` になったことを確認する。
`has_draft: true` のまま残っていれば手順 2 に戻る。

### 5. ユーザーに報告する

解決したハーネス名・セル名・充填したプロンプトの概要を報告する。

## エラー処理

| エラーコード | 対処 |
|------------|------|
| `lock_conflict` | `sugo_status` で最新の `lock_version` を取得し直して再試行 |
| `draft_cells_exist`（`sugo_start` 時） | このスキルを先に完了させてから `sugo_start` を再実行 |
