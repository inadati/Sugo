# サイドバー + ハーネスゴミ箱機能 設計

Date: 2026-06-30

## 概要

ハーネス一覧画面に左サイドバーを追加し、ハーネスの「ゴミ箱」機能を導入する。
削除はソフトデリート（`deleted_at` セット）で、6ヶ月以内は復活可能。

---

## アーキテクチャ

### ルーティング変更

```
Router
  /                  → ShellLayout（新設）
    /                →   HarnessList.vue（既存・更新）
    /trash           →   TrashView.vue（新設）
  /harness/:id       → HarnessView.vue（変更なし）
```

`ShellLayout.vue` がサイドバー（`AppSidebar.vue`）と `<RouterView>` を2カラムで並べる。
HarnessView はシェルレイアウト外なのでサイドバーは表示されない。

### 新設コンポーネント

| ファイル | 役割 |
|---|---|
| `src/layouts/ShellLayout.vue` | サイドバー付き2カラム親レイアウト |
| `src/components/AppSidebar.vue` | 左ナビ（ハーネス・ゴミ箱） |
| `src/views/TrashView.vue` | 削除済みハーネス一覧 |

### アイコン

`@heroicons/vue` を導入する。絵文字はアイコンとして使わない。

---

## バックエンド

### DBマイグレーション

```sql
ALTER TABLE harnesses ADD COLUMN deleted_at TIMESTAMP NULL DEFAULT NULL;
```

- `deleted_at IS NULL` → アクティブ
- `deleted_at IS NOT NULL` → ゴミ箱

既存の `list_harnesses` は `WHERE deleted_at IS NULL` でフィルタする。

### 新規Tauriコマンド

| コマンド | 引数 | 説明 |
|---|---|---|
| `trash_harness` | `harness_id` | `deleted_at = now()` をセット |
| `restore_harness` | `harness_id` | `deleted_at = NULL` に戻す |
| `purge_harness` | `harness_id` | 物理削除 |
| `list_trash` | なし | 削除済み一覧（削除日・残り日数付き） |

### 自動パージ

アプリ起動時に `deleted_at < now() - 180日` のハーネスを物理削除する（6ヶ月 = 180日固定）。

---

## UI詳細

### HarnessList.vue（更新）

- ハーネス行にホバーすると右端に Heroicons の TrashIcon が出現
- クリックで確認ダイアログ：
  > 「"〈ハーネス名〉" をゴミ箱に移動しますか？」
  > [移動する] [キャンセル]
- 確認後に `trash_harness` を呼び、一覧から即座に消える

### AppSidebar.vue

幅140px固定、タイトルバー直下からフル高さ。

```
┌────────────────┐
│  ≡  ハーネス   │  ← アクティブ時ハイライト
│  🗑  ゴミ箱  3 │  ← ゴミ箱内アイテム数バッジ（Heroicons TrashIcon）
└────────────────┘
```

（上記の🗑はイメージ。実装はHeroicons SVGを使う）

### TrashView.vue

各行の表示項目：

```
ハーネス名    削除日: 2026-06-30    あと163日    [復活]  [完全削除]
```

- 残り30日以内は残り日数を赤文字で表示
- 「復活」→ `restore_harness` を呼び、即座にアクティブ一覧へ戻る
- 「完全削除」→ 確認ダイアログ（「完全に削除されます。元に戻せません。」）→ `purge_harness`

---

## エラー処理・エッジケース

### アクティブRunが存在するハーネスのゴミ箱移動

`trash_harness` 呼び出し前にアクティブRunの有無を確認し、存在する場合はブロックする。

- バックエンド: `trash_harness` がアクティブRunを検出した場合はエラーを返す
- フロントエンド: エラーを受け取りダイアログ内またはトースト等でメッセージ表示
  > 「実行中のRunがあるため移動できません」

### 完全削除の確認

物理削除は元に戻せないため、確認ダイアログを必須とする。
