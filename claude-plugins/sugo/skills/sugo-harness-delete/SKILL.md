---
name: sugo-harness-delete
description: |
  Use this skill before calling sugo_delete_harness (moving a harness to the
  trash). Requires confirming with the user first, since deleting a harness
  is a hard-to-reverse structural decision even though the underlying
  operation is a soft delete (restorable from the Sugo GUI's trash view).
version: 0.1.0
tools: []
---

# sugo-harness-delete

`sugo_delete_harness` を呼ぶ前の行動規律。

## 手順

### 1. 削除前にユーザーに確認する

`sugo_delete_harness` はAPI側に誤削除防止の確認機構を持たない（`harness_id`のみで即座に実行される）。
呼び出す前に、対象ハーネスの名前・用途をユーザーに提示し、削除してよいか確認を得ること。
確認なしに削除を実行してはならない。

### 2. 実行中runの有無を事前に確認する

`sugo_status(harness_id)` で `running_runs` を確認する。Running状態のrunが存在し、かつ最終更新から300秒以内であれば
`sugo_delete_harness` は `active_run` エラーで拒否される。実行中runが見つかった場合は、ユーザーにその旨を伝え、
runの完了・停止を待つか、削除を見送るかを確認する。

### 3. 論理削除であることをユーザーに伝えてよい

削除は論理削除（ゴミ箱移動）であり、Sugo GUIのゴミ箱画面から復元できる。この点を伝えることで、
ユーザーが判断しやすくなる。ただし「復元できるから確認不要」とはならない。手順1の確認は省略しない。
