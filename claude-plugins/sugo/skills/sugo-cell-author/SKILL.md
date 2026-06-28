---
name: sugo-cell-author
description: |
  Use this skill when writing or editing a cell prompt in a Sugo harness.
  Apply these rules whenever you call sugo_edit_cell or sugo_update_harness with cell_changes.
version: 0.1.0
tools: []
---

# sugo-cell-author

セルにプロンプトを書く・変更するときの行動規律。

## 手順

### 1. 全体把握を先に行う

`sugo_status()` でハーネス全体の構造を確認してからプロンプトを起草する。
対象セルの役割・前後のセルとの文脈・エッジの接続を理解する。

### 2. 整合性を踏まえてプロンプトを起草する

- 他のセルとの役割分担が重複・矛盾しないか確認する
- プロンプトは「このセルだけを見る者」ではなく「ハーネス全体を知るエージェント」として書く

### 3. 後戻りしにくい判断はユーザーに確認する

以下に該当する場合は実施前に必ずユーザーに確認を求める:

- エッジの追加・削除（フロー構造の変更）
- セルの分割・統合（セル数の増減）
- セルの役割の大幅な変更（プロンプトの方向性が変わる場合）
- `start` セルや `terminal` セルの変更
- 複数セルにまたがる一括変更

### 4. 適用するツール

| 変更内容 | 使用ツール |
|---------|-----------|
| 1セルのみプロンプト変更 | `sugo_edit_cell` |
| 複数セル・エッジを同時変更 | `sugo_update_harness` |
