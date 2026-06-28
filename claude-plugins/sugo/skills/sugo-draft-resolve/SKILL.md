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

対象ハーネスの `sugo_status(harness_id)` で `draft_diff` を確認する。

プロンプト起草のルール:
- ハーネス全体の構造・他のセルとの整合性を踏まえて起草する
- draft セルは必ずプロンプトを充填する（空のままにしない）
- このセルが担う役割・次のセルへの接続・期待する成果物を明確に書く

### 3. `sugo_update_harness` で一括適用する

draft セルすべてに以下を設定して1回のツール呼び出しで送信する:
- `prompt`: 充填したプロンプト
- `status`: `"active"`

`expected_lock_version` は `sugo_status(harness_id)` で取得した値を使う。

```json
{
  "harness_id": "<id>",
  "expected_lock_version": <lock_version>,
  "cell_changes": [
    { "cell_id": "<draft_cell_id>", "prompt": "<充填プロンプト>", "status": "active" }
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
