---
name: sugo-run-stop
description: |
  Use this skill when a run cannot legitimately advance — the current cell's
  outgoing edges all describe the wrong next step — and before calling
  sugo_stop_run. Requires confirming with the user first, since stopping
  discards the run's progress.
version: 0.1.0
tools: []
---

# sugo-run-stop

袋小路に入ったランを終わらせるための行動規律。`sugo_stop_run` を呼ぶ前に読む。

## いつ使うか

**現在のセルから出ているエッジが、どれも誤った前進になるとき。** 典型は、
前の工程の成果物に根本的な誤りが見つかり、その工程まで戻る必要があるのに、
戻る辺が盤面に用意されていない場合である。

このとき `sugo_advance` を「まだしも近い方のエッジ」で呼んではならない。
ランは誤った位置へ進み、以降の全セルが誤った前提の上で実行される。

## いつ使ってはならないか

- **セルのタスクが難しい・時間がかかるだけのとき。** それは停止の理由ではない。
- **ユーザーの確認を取る前。** 手順1を飛ばしてはならない。
- **ハーネス定義そのものを消したいとき。** それは `sugo_delete_harness` の仕事で、
  規律は `sugo-harness-delete` スキルにある。逆に、ランを止めたいだけのときに
  `sugo_delete_harness` を使ってはならない（ハーネスごとゴミ箱へ入ってしまう）。

## 手順

### 1. 止める前にユーザーに確認する

`sugo_stop_run` は `run_id` だけで即座に実行され、API側に誤操作防止の確認機構を持たない。
呼ぶ前に、以下をユーザーに提示して停止の可否を確認すること。

- いまどのセルで止まっているか（セル名）
- そのセルから出ているエッジの一覧と、**なぜどれも選べないのか**
- 停止するとランの進行位置が失われること
- 停止後、同じハーネスを `sugo_start` で最初から引き直せること

**確認なしに停止してはならない。** ランの進行位置は復元できない。

### 2. 停止する

`sugo_stop_run(run_id)` を呼ぶ。`run_id` は自分が受け取った inject のフッタ、
または `sugo_status(harness_id)` の `running_runs[].run_id` から得る。

返り値の `was_in_flight` が `false` のときは、そのランが既に終了していて
何も変更されなかったことを意味する（催促もすでに止まっている）。

### 3. 盤面の穴をユーザーに報告する

袋小路に入ったのは、多くの場合ハーネス設計の穴である。停止を報告する際、
**どのセルからどのセルへ戻る辺が足りなかったか**を具体的に伝えること。
次に同じ場所で詰まらないための情報になる。

辺の追加そのものは盤面の編集であり、`sugo-cell-author` の規律に従う。
後戻りしにくい構造変更はユーザーの承認を得てから行う。
