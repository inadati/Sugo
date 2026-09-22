# セルレイアウトの自動整列と MCP 操作 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ハーネスを開いた時点で読みやすい蛇行レイアウトが自動で適用され、必要なら MCP 経由で座標を読み書き・再整列できるようにする。

**Architecture:** セル座標を SQLite の新テーブル `cell_positions` に置き、sugo-gui と sugo-mcp が同一 DB ファイルを介して共有する。レイアウト計算は sugo-gui 側の純粋関数 `layout.ts` に一本化し、ELK には層割り当てと交差最小化のみを任せ、行への折り返しと蛇行（奇数行の左右反転）は自前で行う。MCP からの全体整列は `cell_positions` の行削除で表現し、レイアウトエンジンを二重に持たない。

**Tech Stack:** Rust (rusqlite / async-trait / rmcp), Tauri 2, Vue 3 + TypeScript, cytoscape, elkjs, vitest, Playwright

**Spec:** `docs/superpowers/specs/2026-09-22-cell-layout-design.md`

## Global Constraints

- 座標は `board_versions.definition_json` に入れない。board version を増やしてはならない。
- 座標書き込みに楽観ロック（`lock_version`）を掛けない。後勝ちでよい。
- レイアウト計算は決定的であること。`Math.random()` や時刻に依存してはならない。
- 既存 localStorage キー `sugo:layout:<harnessId>` からの移行は行わない。読まない、書かない。
- 新規依存は `elkjs` のみ（`^0.12.0`）。`cytoscape-elk` は使わない。
- `layout.ts` は cytoscape・Tauri・DB のいずれにも import 依存してはならない。
- ノード寸法の既定値: 幅 150px（`BoardGraph.vue` の STYLES `width: "150px"`）、高さは `buildHeight(label)` の算出値。
- 座標の単位はセルの**中心**（cytoscape の `node.position()` と同じ）。ELK は左上基準なので境界で変換する。
- Rust のエラーは `CoreError` を返し、Tauri 境界では既存の `map_core_error` を通す。

---

### Task 1: `cell_positions` テーブルとリポジトリ

**Files:**
- Create: `sugo-core/src/domain/cell_position.rs`
- Create: `sugo-core/src/ports/cell_position_repository.rs`
- Modify: `sugo-core/src/domain/mod.rs`
- Modify: `sugo-core/src/ports/mod.rs`
- Modify: `sugo-infra/src/sqlite/schema.rs:56`（`runs` テーブル定義の直後）
- Create: `sugo-infra/src/sqlite/cell_position_repository.rs`
- Modify: `sugo-infra/src/sqlite/mod.rs`
- Test: `sugo-infra/tests/sqlite_cell_position_repository.rs`

**Interfaces:**
- Consumes: なし（最初のタスク）
- Produces:
  - `sugo_core::domain::cell_position::CellPosition { cell_id: String, x: f64, y: f64 }`
  - `sugo_core::ports::cell_position_repository::CellPositionRepository` — `async fn list(&self, harness_id: &str) -> Result<Vec<CellPosition>, CoreError>` / `async fn replace_all(&self, harness_id: &str, positions: &[CellPosition]) -> Result<(), CoreError>` / `async fn upsert(&self, harness_id: &str, positions: &[CellPosition]) -> Result<(), CoreError>` / `async fn clear(&self, harness_id: &str) -> Result<(), CoreError>`
  - `sugo_infra::sqlite::SqliteCellPositionRepository::new(conn: Mutex<rusqlite::Connection>)`

`replace_all` と `upsert` を分ける理由: GUI の保存は「現在のノード集合で全置換」（削除済みセルの座標を残さないため）だが、MCP の `sugo_set_layout` は「指定したセルだけ上書き」であり他セルを消してはならない。

- [ ] **Step 1: ドメイン型を作る**

`sugo-core/src/domain/cell_position.rs` を作成する。

```rust
//! セルの表示座標。盤面の意味論ではなく見た目の情報であり、
//! `BoardDefinition` とは別に永続化される。

/// 1セル分の表示座標。`x`/`y` はセル中心の座標。
#[derive(Debug, Clone, PartialEq)]
pub struct CellPosition {
    pub cell_id: String,
    pub x: f64,
    pub y: f64,
}
```

`sugo-core/src/domain/mod.rs` に `pub mod cell_position;` を追加する。

- [ ] **Step 2: ポート trait を作る**

`sugo-core/src/ports/cell_position_repository.rs` を作成する。

```rust
//! Output port for cell display-position persistence.

use crate::domain::cell_position::CellPosition;
use crate::error::CoreError;
use async_trait::async_trait;

#[async_trait]
pub trait CellPositionRepository: Send + Sync {
    /// ハーネスの全セル座標を返す。未登録なら空 Vec。
    async fn list(&self, harness_id: &str) -> Result<Vec<CellPosition>, CoreError>;

    /// ハーネスの座標を与えられた集合で全置換する。
    ///
    /// 削除・挿入を単一トランザクションで行うため、盤面から消えたセルの
    /// 座標が残留しない。GUI の保存はこちらを使う。
    async fn replace_all(
        &self,
        harness_id: &str,
        positions: &[CellPosition],
    ) -> Result<(), CoreError>;

    /// 指定されたセルの座標だけを上書きする。他セルの座標は保持する。
    async fn upsert(
        &self,
        harness_id: &str,
        positions: &[CellPosition],
    ) -> Result<(), CoreError>;

    /// ハーネスの座標を全削除する。次の描画で自動レイアウトが走る。
    async fn clear(&self, harness_id: &str) -> Result<(), CoreError>;
}
```

`sugo-core/src/ports/mod.rs` に `pub mod cell_position_repository;` を追加する。

- [ ] **Step 3: 失敗するテストを書く**

`sugo-infra/tests/sqlite_cell_position_repository.rs` を作成する。

```rust
//! Integration tests for [`SqliteCellPositionRepository`] driven through the
//! public [`CellPositionRepository`] port.

use std::sync::Mutex;
use sugo_core::domain::cell_position::CellPosition;
use sugo_core::ports::cell_position_repository::CellPositionRepository;
use sugo_infra::sqlite::{SqliteCellPositionRepository, SqliteHarnessRepository};

/// スキーマ適用済みの一時 DB を開き、座標リポジトリを返す。
///
/// スキーマは `SqliteHarnessRepository::open` が適用するため、同じファイルを
/// 別コネクションで開き直す（sugo-mcp / sugo-gui と同じ組み立て方）。
fn repo() -> (tempfile::TempDir, SqliteCellPositionRepository) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("sugo.db");
    let path_str = path.to_string_lossy().into_owned();
    SqliteHarnessRepository::open(&path_str).expect("schema applied");
    let conn = rusqlite::Connection::open(&path_str).expect("open");
    (dir, SqliteCellPositionRepository::new(Mutex::new(conn)))
}

fn pos(cell_id: &str, x: f64, y: f64) -> CellPosition {
    CellPosition { cell_id: cell_id.into(), x, y }
}

#[tokio::test]
async fn list_returns_empty_for_unknown_harness() {
    let (_d, r) = repo();
    assert_eq!(r.list("nope").await.expect("list ok"), vec![]);
}

#[tokio::test]
async fn replace_all_then_list_round_trips() {
    let (_d, r) = repo();
    r.replace_all("h1", &[pos("c1", 10.0, 20.0), pos("c2", 30.5, 40.5)])
        .await
        .expect("replace ok");

    let mut got = r.list("h1").await.expect("list ok");
    got.sort_by(|a, b| a.cell_id.cmp(&b.cell_id));
    assert_eq!(got, vec![pos("c1", 10.0, 20.0), pos("c2", 30.5, 40.5)]);
}

#[tokio::test]
async fn replace_all_drops_positions_of_removed_cells() {
    let (_d, r) = repo();
    r.replace_all("h1", &[pos("c1", 1.0, 1.0), pos("c2", 2.0, 2.0)])
        .await
        .expect("replace ok");
    // c2 が盤面から消えた状態で保存し直す
    r.replace_all("h1", &[pos("c1", 9.0, 9.0)])
        .await
        .expect("replace ok");

    assert_eq!(r.list("h1").await.expect("list ok"), vec![pos("c1", 9.0, 9.0)]);
}

#[tokio::test]
async fn replace_all_does_not_touch_other_harnesses() {
    let (_d, r) = repo();
    r.replace_all("h1", &[pos("c1", 1.0, 1.0)]).await.expect("ok");
    r.replace_all("h2", &[pos("c9", 5.0, 5.0)]).await.expect("ok");

    assert_eq!(r.list("h2").await.expect("list ok"), vec![pos("c9", 5.0, 5.0)]);
}

#[tokio::test]
async fn upsert_overwrites_named_cells_and_keeps_the_rest() {
    let (_d, r) = repo();
    r.replace_all("h1", &[pos("c1", 1.0, 1.0), pos("c2", 2.0, 2.0)])
        .await
        .expect("ok");
    r.upsert("h1", &[pos("c2", 77.0, 88.0)]).await.expect("upsert ok");

    let mut got = r.list("h1").await.expect("list ok");
    got.sort_by(|a, b| a.cell_id.cmp(&b.cell_id));
    assert_eq!(got, vec![pos("c1", 1.0, 1.0), pos("c2", 77.0, 88.0)]);
}

#[tokio::test]
async fn clear_removes_only_the_named_harness() {
    let (_d, r) = repo();
    r.replace_all("h1", &[pos("c1", 1.0, 1.0)]).await.expect("ok");
    r.replace_all("h2", &[pos("c9", 5.0, 5.0)]).await.expect("ok");

    r.clear("h1").await.expect("clear ok");

    assert_eq!(r.list("h1").await.expect("list ok"), vec![]);
    assert_eq!(r.list("h2").await.expect("list ok"), vec![pos("c9", 5.0, 5.0)]);
}
```

`tempfile` と `tokio` が `sugo-infra` の dev-dependencies に無ければ追加する。
既存 `sugo-infra/tests/sqlite_repository.rs` がどちらを使っているか確認し、同じ流儀に合わせること。

- [ ] **Step 4: テストが失敗することを確認する**

Run: `cargo test -p sugo-infra --test sqlite_cell_position_repository`
Expected: コンパイルエラー。`SqliteCellPositionRepository` が存在しない。

- [ ] **Step 5: スキーマにテーブルを追加する**

`sugo-infra/src/sqlite/schema.rs` の `SCHEMA` 定数、`runs` テーブル定義の後ろに追記する。

```sql
CREATE TABLE IF NOT EXISTS cell_positions (
  harness_id TEXT NOT NULL REFERENCES harnesses(id),
  cell_id    TEXT NOT NULL,
  x          REAL NOT NULL,
  y          REAL NOT NULL,
  PRIMARY KEY (harness_id, cell_id)
);
```

同ファイル冒頭のドキュメンテーションコメントにも `cell_positions` を追記する。
座標は盤面の意味論ではなく表示情報であるため `board_versions` に入れない旨を一行添える。

- [ ] **Step 6: リポジトリを実装する**

`sugo-infra/src/sqlite/cell_position_repository.rs` を作成する。

```rust
//! SQLite-backed CellPositionRepository.
//!
//! 座標は表示上の情報であり、`board_versions` の不変 JSON とは独立に
//! 保持される。版は上がらず、楽観ロックも掛けない（後勝ち）。

use async_trait::async_trait;
use std::sync::Mutex;
use sugo_core::domain::cell_position::CellPosition;
use sugo_core::error::CoreError;
use sugo_core::ports::cell_position_repository::CellPositionRepository;

fn map_err(e: rusqlite::Error) -> CoreError {
    CoreError::Storage(e.to_string())
}

pub struct SqliteCellPositionRepository {
    conn: Mutex<rusqlite::Connection>,
}

impl SqliteCellPositionRepository {
    pub fn new(conn: Mutex<rusqlite::Connection>) -> Self {
        Self { conn }
    }
}

#[async_trait]
impl CellPositionRepository for SqliteCellPositionRepository {
    async fn list(&self, harness_id: &str) -> Result<Vec<CellPosition>, CoreError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn
            .prepare("SELECT cell_id, x, y FROM cell_positions WHERE harness_id = ?1")
            .map_err(map_err)?;
        let rows = stmt
            .query_map([harness_id], |row| {
                Ok(CellPosition {
                    cell_id: row.get(0)?,
                    x: row.get(1)?,
                    y: row.get(2)?,
                })
            })
            .map_err(map_err)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(map_err)?);
        }
        Ok(out)
    }

    async fn replace_all(
        &self,
        harness_id: &str,
        positions: &[CellPosition],
    ) -> Result<(), CoreError> {
        let mut conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let tx = conn.transaction().map_err(map_err)?;
        tx.execute("DELETE FROM cell_positions WHERE harness_id = ?1", [harness_id])
            .map_err(map_err)?;
        for p in positions {
            tx.execute(
                "INSERT INTO cell_positions (harness_id, cell_id, x, y) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![harness_id, p.cell_id, p.x, p.y],
            )
            .map_err(map_err)?;
        }
        tx.commit().map_err(map_err)?;
        Ok(())
    }

    async fn upsert(
        &self,
        harness_id: &str,
        positions: &[CellPosition],
    ) -> Result<(), CoreError> {
        let mut conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let tx = conn.transaction().map_err(map_err)?;
        for p in positions {
            tx.execute(
                "INSERT INTO cell_positions (harness_id, cell_id, x, y) VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT(harness_id, cell_id) DO UPDATE SET x = ?3, y = ?4",
                rusqlite::params![harness_id, p.cell_id, p.x, p.y],
            )
            .map_err(map_err)?;
        }
        tx.commit().map_err(map_err)?;
        Ok(())
    }

    async fn clear(&self, harness_id: &str) -> Result<(), CoreError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute("DELETE FROM cell_positions WHERE harness_id = ?1", [harness_id])
            .map_err(map_err)?;
        Ok(())
    }
}
```

`sugo-infra/src/sqlite/mod.rs` に以下を追加する。

```rust
pub mod cell_position_repository;
pub use cell_position_repository::SqliteCellPositionRepository;
```

- [ ] **Step 7: テストが通ることを確認する**

Run: `cargo test -p sugo-infra --test sqlite_cell_position_repository`
Expected: 6件すべて PASS

- [ ] **Step 8: 既存テストの回帰がないことを確認する**

Run: `cargo test --workspace`
Expected: 全件 PASS

- [ ] **Step 9: コミット**

```bash
git add sugo-core/src/domain/cell_position.rs sugo-core/src/domain/mod.rs \
        sugo-core/src/ports/cell_position_repository.rs sugo-core/src/ports/mod.rs \
        sugo-infra/src/sqlite/schema.rs sugo-infra/src/sqlite/cell_position_repository.rs \
        sugo-infra/src/sqlite/mod.rs sugo-infra/tests/sqlite_cell_position_repository.rs
git commit -m "feat: セル表示座標を保持する cell_positions テーブルとリポジトリを追加する"
```

---

### Task 2: ハーネス完全削除で座標も削除する

**Files:**
- Modify: `sugo-infra/src/sqlite/repository.rs:328-336`（`purge` のトランザクション内）
- Test: `sugo-infra/tests/sqlite_repository.rs`

**Interfaces:**
- Consumes: Task 1 の `cell_positions` テーブル
- Produces: なし（既存 `HarnessRepository::purge` の挙動拡張のみ）

`cell_positions.harness_id` は `harnesses(id)` への外部キーを持つため、座標を残したままハーネス行を削除すると
`PRAGMA foreign_keys = ON` の下で削除が失敗する。これは機能追加ではなく Task 1 が持ち込んだ不整合の解消である。

- [ ] **Step 1: 失敗するテストを書く**

`sugo-infra/tests/sqlite_repository.rs` の末尾に追加する。
既存テストのヘルパー（`helpers::board` など）と、ハーネス作成〜trash〜purge の手順は
同ファイル内の既存 purge テストに倣うこと。

```rust
#[tokio::test]
async fn purge_also_removes_cell_positions() {
    use std::sync::Mutex;
    use sugo_core::domain::cell_position::CellPosition;
    use sugo_core::ports::cell_position_repository::CellPositionRepository;
    use sugo_infra::sqlite::SqliteCellPositionRepository;

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("sugo.db").to_string_lossy().into_owned();
    let repo = SqliteHarnessRepository::open(&path).expect("open");

    // 既存テストと同じ手順でハーネスを1件作り、trash してから purge する。
    // （作成・trash の具体的な呼び出しは同ファイル内の既存 purge テストを参照）
    let harness_id = create_trashed_harness(&repo).await;

    let conn = rusqlite::Connection::open(&path).expect("open");
    let pos_repo = SqliteCellPositionRepository::new(Mutex::new(conn));
    pos_repo
        .replace_all(&harness_id, &[CellPosition { cell_id: "c1".into(), x: 1.0, y: 2.0 }])
        .await
        .expect("replace ok");

    repo.purge(&harness_id).await.expect("purge ok");

    assert_eq!(pos_repo.list(&harness_id).await.expect("list ok"), vec![]);
}
```

`create_trashed_harness` はこのファイルに既存のヘルパーが無ければ、既存 purge テストの
本体をそのまま切り出して作ること。既存テストの重複コードを減らす目的も兼ねる。

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cargo test -p sugo-infra --test sqlite_repository purge_also_removes_cell_positions`
Expected: FAIL。座標が残っている、または外部キー制約で purge 自体が失敗する。

- [ ] **Step 3: purge にカスケード削除を足す**

`sugo-infra/src/sqlite/repository.rs` の `purge` 内、`DELETE FROM board_versions ...`（現 328 行目）の直前に追加する。

```rust
tx.execute("DELETE FROM cell_positions WHERE harness_id = ?1", [id])
    .map_err(map_err)?;
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cargo test -p sugo-infra`
Expected: 全件 PASS

- [ ] **Step 5: コミット**

```bash
git add sugo-infra/src/sqlite/repository.rs sugo-infra/tests/sqlite_repository.rs
git commit -m "fix: ハーネス完全削除時にセル座標も削除する"
```

---

### Task 3: Tauri コマンド3本

**Files:**
- Modify: `sugo-gui/src-tauri/src/state.rs`
- Modify: `sugo-gui/src-tauri/src/dto.rs`
- Modify: `sugo-gui/src-tauri/src/commands.rs`
- Modify: `sugo-gui/src-tauri/src/lib.rs:49-72`

**Interfaces:**
- Consumes: `CellPositionRepository`, `SqliteCellPositionRepository::new`（Task 1）
- Produces: Tauri コマンド 3 本
  - `get_cell_positions(harness_id: String) -> Result<Vec<CellPositionDto>, String>`
  - `save_cell_positions(harness_id: String, positions: Vec<CellPositionDto>) -> Result<(), String>`
  - `clear_cell_positions(harness_id: String) -> Result<(), String>`
  - `CellPositionDto { cell_id: String, x: f64, y: f64 }`（serde でキャメルケース変換はしない。既存 DTO に合わせスネークケースのまま）

- [ ] **Step 1: DTO を追加する**

`sugo-gui/src-tauri/src/dto.rs` に追加する。既存 DTO の derive 構成（`serde::Serialize` など）を確認し同じものを付けること。
`save_cell_positions` の引数にも使うため `Deserialize` も必要。

```rust
/// セルの表示座標。x/y はセル中心の座標。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CellPositionDto {
    pub cell_id: String,
    pub x: f64,
    pub y: f64,
}
```

- [ ] **Step 2: AppState に座標リポジトリを足す**

`sugo-gui/src-tauri/src/state.rs` を次のように変更する。

```rust
use std::sync::{Arc, Mutex};
use sugo_infra::sqlite::cell_position_repository::SqliteCellPositionRepository;
use sugo_infra::sqlite::repository::SqliteHarnessRepository;
use sugo_infra::sqlite::run_repository::SqliteRunRepository;

pub struct AppState {
    pub repo: Arc<SqliteHarnessRepository>,
    pub run_repo: Arc<SqliteRunRepository>,
    pub pos_repo: Arc<SqliteCellPositionRepository>,
}

impl AppState {
    pub fn new(db_path: &str) -> Result<Self, String> {
        let repo = SqliteHarnessRepository::open(db_path).map_err(|e| e.to_string())?;
        let conn = rusqlite::Connection::open(db_path).map_err(|e| e.to_string())?;
        let run_repo = SqliteRunRepository::new(Mutex::new(conn));
        // 座標用にもう1本コネクションを開く。run_repo と同じく
        // スキーマ適用は SqliteHarnessRepository::open が済ませている。
        let pos_conn = rusqlite::Connection::open(db_path).map_err(|e| e.to_string())?;
        let pos_repo = SqliteCellPositionRepository::new(Mutex::new(pos_conn));
        Ok(Self {
            repo: Arc::new(repo),
            run_repo: Arc::new(run_repo),
            pos_repo: Arc::new(pos_repo),
        })
    }
}
```

- [ ] **Step 3: 失敗するテストを書く**

`sugo-gui/src-tauri/src/commands.rs` の `#[cfg(test)] mod tests` に追加する。
既存コマンドのテストが `*_inner` 関数を直接呼ぶ形になっているので、それに倣う。

```rust
#[tokio::test]
async fn cell_positions_round_trip_through_inner_fns() {
    use std::sync::Mutex;
    use sugo_infra::sqlite::cell_position_repository::SqliteCellPositionRepository;

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("sugo.db").to_string_lossy().into_owned();
    sugo_infra::sqlite::SqliteHarnessRepository::open(&path).expect("schema");
    let conn = rusqlite::Connection::open(&path).expect("open");
    let pos_repo = SqliteCellPositionRepository::new(Mutex::new(conn));

    save_cell_positions_inner(
        &pos_repo,
        "h1".into(),
        vec![CellPositionDto { cell_id: "c1".into(), x: 3.0, y: 4.0 }],
    )
    .await
    .expect("save ok");

    let got = get_cell_positions_inner(&pos_repo, "h1".into()).await.expect("get ok");
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].cell_id, "c1");
    assert_eq!(got[0].x, 3.0);
    assert_eq!(got[0].y, 4.0);

    clear_cell_positions_inner(&pos_repo, "h1".into()).await.expect("clear ok");
    assert!(get_cell_positions_inner(&pos_repo, "h1".into()).await.expect("get ok").is_empty());
}
```

- [ ] **Step 4: テストが失敗することを確認する**

Run: `cargo test -p sugo-gui cell_positions_round_trip`
Expected: コンパイルエラー。`save_cell_positions_inner` が存在しない。

クレート名が `sugo-gui` でない場合は `sugo-gui/src-tauri/Cargo.toml` の `[package] name` を確認して読み替えること。

- [ ] **Step 5: コマンドを実装する**

`sugo-gui/src-tauri/src/commands.rs` に追加する。冒頭の `use crate::dto::{...}` に `CellPositionDto` を加え、
`use sugo_core::domain::cell_position::CellPosition;` と
`use sugo_core::ports::cell_position_repository::CellPositionRepository;` を追加する。

```rust
/// ハーネスのセル表示座標を取得する。未登録なら空配列。
#[tauri::command]
pub async fn get_cell_positions(
    state: State<'_, AppState>,
    harness_id: String,
) -> Result<Vec<CellPositionDto>, String> {
    get_cell_positions_inner(state.pos_repo.as_ref(), harness_id).await
}

async fn get_cell_positions_inner(
    repo: &dyn CellPositionRepository,
    harness_id: String,
) -> Result<Vec<CellPositionDto>, String> {
    let positions = repo.list(&harness_id).await.map_err(map_core_error)?;
    Ok(positions
        .into_iter()
        .map(|p| CellPositionDto { cell_id: p.cell_id, x: p.x, y: p.y })
        .collect())
}

/// ハーネスのセル表示座標を全置換する。
///
/// 盤面から消えたセルの座標が残留しないよう、削除と挿入を単一トランザクション
/// で行う `replace_all` を使う（`upsert` ではない）。
#[tauri::command]
pub async fn save_cell_positions(
    state: State<'_, AppState>,
    harness_id: String,
    positions: Vec<CellPositionDto>,
) -> Result<(), String> {
    save_cell_positions_inner(state.pos_repo.as_ref(), harness_id, positions).await
}

async fn save_cell_positions_inner(
    repo: &dyn CellPositionRepository,
    harness_id: String,
    positions: Vec<CellPositionDto>,
) -> Result<(), String> {
    let mapped: Vec<CellPosition> = positions
        .into_iter()
        .map(|p| CellPosition { cell_id: p.cell_id, x: p.x, y: p.y })
        .collect();
    repo.replace_all(&harness_id, &mapped).await.map_err(map_core_error)
}

/// ハーネスのセル表示座標を全削除する。次の描画で自動レイアウトが走る。
#[tauri::command]
pub async fn clear_cell_positions(
    state: State<'_, AppState>,
    harness_id: String,
) -> Result<(), String> {
    clear_cell_positions_inner(state.pos_repo.as_ref(), harness_id).await
}

async fn clear_cell_positions_inner(
    repo: &dyn CellPositionRepository,
    harness_id: String,
) -> Result<(), String> {
    repo.clear(&harness_id).await.map_err(map_core_error)
}
```

- [ ] **Step 6: コマンドを登録する**

`sugo-gui/src-tauri/src/lib.rs` の `tauri::generate_handler![...]`（現 49-72 行目）の末尾、
`move_harness_to_folder,` の後に追加する。

```rust
            get_cell_positions,
            save_cell_positions,
            clear_cell_positions,
```

同ファイル冒頭の `use crate::commands::{...}` にも3本を追加する。

- [ ] **Step 7: テストが通ることを確認する**

Run: `cargo test -p sugo-gui`
Expected: 全件 PASS

- [ ] **Step 8: コミット**

```bash
git add sugo-gui/src-tauri/src/state.rs sugo-gui/src-tauri/src/dto.rs \
        sugo-gui/src-tauri/src/commands.rs sugo-gui/src-tauri/src/lib.rs
git commit -m "feat: セル座標の取得・保存・削除を行う Tauri コマンドを追加する"
```

---

### Task 4: GUI の座標保存先を localStorage から DB へ移す

レイアウト計算はこの時点では dagre のまま据え置く。保存先の移行とレイアウトの刷新を
同時に行うと、不具合が出たときに原因を切り分けられなくなるため。

**Files:**
- Create: `sugo-gui/src/lib/positions.ts`
- Create: `sugo-gui/src/lib/positions.test.ts`
- Modify: `sugo-gui/src/components/BoardGraph.vue:122-170`（レイアウト永続化ブロック）
- Modify: `sugo-gui/src/components/BoardGraph.vue:430-456`（`placeNodes`）
- Modify: `sugo-gui/src/components/BoardGraph.vue:503`, `:538`, `:609`（呼び出し側）
- Modify: `sugo-gui/src/components/BoardGraph.test.ts`

**Interfaces:**
- Consumes: Tauri コマンド 3 本（Task 3）
- Produces:
  - `sugo-gui/src/lib/positions.ts` の `export type PositionMap = Record<string, { x: number; y: number }>`
  - `export async function loadPositions(harnessId: string): Promise<PositionMap>`
  - `export async function savePositions(harnessId: string, positions: PositionMap): Promise<void>`
  - `export async function clearPositions(harnessId: string): Promise<void>`

- [ ] **Step 1: 失敗するテストを書く**

`sugo-gui/src/lib/positions.test.ts` を作成する。

```ts
import { describe, it, expect, vi, beforeEach } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

import { loadPositions, savePositions, clearPositions } from "./positions";

beforeEach(() => invoke.mockReset());

describe("loadPositions", () => {
  it("コマンドの配列を id をキーにしたマップへ変換する", async () => {
    invoke.mockResolvedValue([
      { cell_id: "c1", x: 1, y: 2 },
      { cell_id: "c2", x: 3, y: 4 },
    ]);
    expect(await loadPositions("h1")).toEqual({ c1: { x: 1, y: 2 }, c2: { x: 3, y: 4 } });
    expect(invoke).toHaveBeenCalledWith("get_cell_positions", { harnessId: "h1" });
  });

  it("異常な座標（NaN・非有限・極端に大きい値）を捨てる", async () => {
    invoke.mockResolvedValue([
      { cell_id: "ok", x: 1, y: 2 },
      { cell_id: "nan", x: Number.NaN, y: 0 },
      { cell_id: "inf", x: 0, y: Number.POSITIVE_INFINITY },
      { cell_id: "huge", x: 999999, y: 0 },
    ]);
    expect(await loadPositions("h1")).toEqual({ ok: { x: 1, y: 2 } });
  });
});

describe("savePositions", () => {
  it("マップを配列へ変換してコマンドへ渡す", async () => {
    invoke.mockResolvedValue(undefined);
    await savePositions("h1", { c1: { x: 1, y: 2 } });
    expect(invoke).toHaveBeenCalledWith("save_cell_positions", {
      harnessId: "h1",
      positions: [{ cell_id: "c1", x: 1, y: 2 }],
    });
  });
});

describe("clearPositions", () => {
  it("コマンドを呼ぶ", async () => {
    invoke.mockResolvedValue(undefined);
    await clearPositions("h1");
    expect(invoke).toHaveBeenCalledWith("clear_cell_positions", { harnessId: "h1" });
  });
});
```

異常値を捨てる仕様は既存 `BoardGraph.vue:135-148` の `isSaneCoord` をそのまま引き継ぐ。
過去の不具合で保存された異常座標が `cy.fit()` を壊した経緯があるため、保存先が DB に変わっても防御は残す。

Tauri の `invoke` が引数名をキャメルケースへ変換する規約に依存しているので、
実装後に実機で `harnessId` / `harness_id` のどちらが通るかを必ず確認すること。
異なっていた場合はテストと実装の両方を実際に通る側へ揃える。

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd sugo-gui && npx vitest run src/lib/positions.test.ts`
Expected: FAIL。`./positions` が解決できない。

- [ ] **Step 3: positions.ts を実装する**

`sugo-gui/src/lib/positions.ts` を作成する。

```ts
// セル表示座標の永続化。Tauri コマンド越しに SQLite の cell_positions を読み書きする。
//
// 以前は localStorage に保存していたが、sugo-mcp（別プロセスの Rust サーバー）が
// 座標を読み書きできるよう DB へ移した。localStorage からの移行は行わない
// （座標は盤面から再計算でき、そこに残っているのは解消対象の横一列配置であるため）。
import { invoke } from "@tauri-apps/api/core";

export type PositionMap = Record<string, { x: number; y: number }>;

type CellPositionDto = { cell_id: string; x: number; y: number };

// 過去の不具合で異常な座標が保存されると cy.fit() が極端にズームアウトし、
// 他のノードが実質見えなくなる。そのセルだけ「未保存」扱いにして自動配置へ
// 回すことで自己修復する。
const MAX_SANE_COORD = 20000;

function isSaneCoord(x: number, y: number): boolean {
  return (
    Number.isFinite(x) && Math.abs(x) <= MAX_SANE_COORD &&
    Number.isFinite(y) && Math.abs(y) <= MAX_SANE_COORD
  );
}

export async function loadPositions(harnessId: string): Promise<PositionMap> {
  const rows = await invoke<CellPositionDto[]>("get_cell_positions", { harnessId });
  const out: PositionMap = {};
  for (const r of rows) {
    if (isSaneCoord(r.x, r.y)) out[r.cell_id] = { x: r.x, y: r.y };
  }
  return out;
}

export async function savePositions(harnessId: string, positions: PositionMap): Promise<void> {
  const rows: CellPositionDto[] = Object.entries(positions).map(([cell_id, p]) => ({
    cell_id,
    x: p.x,
    y: p.y,
  }));
  await invoke("save_cell_positions", { harnessId, positions: rows });
}

export async function clearPositions(harnessId: string): Promise<void> {
  await invoke("clear_cell_positions", { harnessId });
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd sugo-gui && npx vitest run src/lib/positions.test.ts`
Expected: 5件 PASS

- [ ] **Step 5: BoardGraph.vue を差し替える**

1. `BoardGraph.vue:122-170` の永続化ブロック（`lsKey` / `savePositions` / `MAX_SANE_COORD` /
   `isSaneCoord` / `loadPositions` / `applyPositions`）から、`applyPositions` を除く全てを削除する。
2. import に追加する。

```ts
import {
  loadPositions,
  savePositions as persistPositions,
  type PositionMap,
} from "../lib/positions";
```

既存のローカル型宣言 `type PositionMap = Record<string, { x: number; y: number }>;`（現 90 行目付近）は削除し、
`positions.ts` の型を使う。

3. 現在の座標を DB へ書く薄い関数を置く。

```ts
async function saveCurrentPositions() {
  if (!cy) return;
  const pos: PositionMap = {};
  cy.nodes().forEach((n) => { pos[n.id()] = { ...n.position() }; });
  await persistPositions(props.harnessId, pos);
}
```

4. `placeNodes` を async にする。

```ts
async function placeNodes() {
  if (!cy) return;
  const saved = await loadPositions(props.harnessId);
  // placeNodes は await をまたぐため、その間に別のハーネスへ遷移して
  // cy が破棄されている可能性がある。再確認する。
  if (!cy) return;

  const missingIds = props.cells.filter((c) => saved[c.id] == null).map((c) => c.id);
  const hasAnySaved = Object.keys(saved).length > 0;

  if (hasAnySaved && missingIds.length < props.cells.length) {
    applyPositions(saved);
    if (missingIds.length > 0) {
      placeNewNodes(saved, missingIds);
      await saveCurrentPositions();
    }
    fitView();
    computeMarkerPositions();
  } else {
    const layout = cy.layout(DAGRE_OPTIONS);
    layout.one("layoutstop", () => {
      void saveCurrentPositions();
      fitView();
      computeMarkerPositions();
    });
    layout.run();
  }
}
```

5. 呼び出し側3箇所を直す。
   - 現 503 行目のドラッグ確定ハンドラ内 `savePositions();` → `void saveCurrentPositions();`
   - 現 538 行目 `requestAnimationFrame(() => placeNodes());` → `requestAnimationFrame(() => void placeNodes());`
   - 現 609 行目 `placeNodes();`（`refresh` 内） → `void placeNodes();`

- [ ] **Step 6: BoardGraph.test.ts を直す**

既存テストは `vi.mock("cytoscape", ...)` で cytoscape をモックしている。
`../lib/positions` のモックを追加する。

```ts
vi.mock("../lib/positions", () => ({
  loadPositions: vi.fn(async () => ({})),
  savePositions: vi.fn(async () => {}),
  clearPositions: vi.fn(async () => {}),
}));
```

`placeNodes` が async になったため、マウント後に `await flushPromises()`
（`@vue/test-utils` から import）を挟まないと配置が完了しない。
既存テストのうち配置結果に依存するものへ追加する。

- [ ] **Step 7: テストが通ることを確認する**

Run: `cd sugo-gui && npm test`
Expected: 全件 PASS

- [ ] **Step 8: 実機で確認する**

Run: `cd sugo-gui && npm run tauri dev`

確認内容: 既存ハーネスを開き、ノードを1つドラッグして移動し、一覧へ戻ってから開き直す。
移動後の位置が保たれていること。localStorage ではなく DB に入っていることを
`sqlite3 <db_path> "SELECT * FROM cell_positions LIMIT 5;"` で確認する。
DB パスは `sugo_infra::paths::default_db_path()` の値。

- [ ] **Step 9: コミット**

```bash
git add sugo-gui/src/lib/positions.ts sugo-gui/src/lib/positions.test.ts \
        sugo-gui/src/components/BoardGraph.vue sugo-gui/src/components/BoardGraph.test.ts
git commit -m "refactor: セル座標の保存先を localStorage から SQLite へ移す"
```

---

### Task 5: 蛇行レイアウトエンジン

**Files:**
- Modify: `sugo-gui/package.json`（`elkjs` 追加）
- Create: `sugo-gui/src/lib/layout.ts`
- Create: `sugo-gui/src/lib/layout.test.ts`

**Interfaces:**
- Consumes: `PositionMap`（Task 4 の `positions.ts`）
- Produces:
  - `export type LayoutNode = { id: string; width: number; height: number }`
  - `export type LayoutEdge = { from: string; to: string }`
  - `export type ArrangeResult = { positions: PositionMap; width: number; height: number }`
  - `export type PlacedNode = { id: string; x: number; y: number; width: number; height: number }`（ELK の左上基準座標）
  - `export function groupIntoLayers(placed: PlacedNode[]): PlacedNode[][]`
  - `export function arrangeRows(layers: PlacedNode[][], layersPerRow: number, serpentine?: boolean): ArrangeResult`
  - `export function chooseLayersPerRow(layers: PlacedNode[][], targetAspect?: number): number`
  - `export async function computeLayout(nodes: LayoutNode[], edges: LayoutEdge[], targetAspect?: number): Promise<PositionMap>`

`arrangeRows` の返す `positions` はセル**中心**座標（cytoscape の `node.position()` と同じ）。
`PlacedNode` は ELK 由来の**左上**座標。この変換は `arrangeRows` の内部で行う。

- [ ] **Step 1: elkjs を追加する**

Run: `cd sugo-gui && npm install elkjs@^0.12.0`

- [ ] **Step 2: 失敗するテストを書く**

`sugo-gui/src/lib/layout.test.ts` を作成する。elkjs は Node 上で動くためモックしない。

```ts
import { describe, it, expect } from "vitest";
import {
  computeLayout,
  groupIntoLayers,
  arrangeRows,
  type LayoutNode,
  type LayoutEdge,
  type PlacedNode,
} from "./layout";
import type { PositionMap } from "./positions";

const NODE_W = 150;
const NODE_H = 70;

/** n マスの一本鎖を作る。 */
function chain(n: number): { nodes: LayoutNode[]; edges: LayoutEdge[] } {
  const nodes = Array.from({ length: n }, (_, i) => ({
    id: `c${i}`,
    width: NODE_W,
    height: NODE_H,
  }));
  const edges = Array.from({ length: n - 1 }, (_, i) => ({ from: `c${i}`, to: `c${i + 1}` }));
  return { nodes, edges };
}

/** 2線分が交差するかを判定する（端点の共有は交差とみなさない）。 */
function segmentsCross(
  a1: { x: number; y: number }, a2: { x: number; y: number },
  b1: { x: number; y: number }, b2: { x: number; y: number },
): boolean {
  const d = (p: { x: number; y: number }, q: { x: number; y: number }, r: { x: number; y: number }) =>
    (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
  const d1 = d(b1, b2, a1), d2 = d(b1, b2, a2);
  const d3 = d(a1, a2, b1), d4 = d(a1, a2, b2);
  return ((d1 > 0 && d2 < 0) || (d1 < 0 && d2 > 0)) && ((d3 > 0 && d4 < 0) || (d3 < 0 && d4 > 0));
}

/** 座標マップとエッジから、直線で描いた場合の交差数を数える。 */
function countCrossings(positions: PositionMap, edges: LayoutEdge[]): number {
  const segs = edges
    .map((e) => ({ a: positions[e.from], b: positions[e.to] }))
    .filter((s) => s.a != null && s.b != null);
  let n = 0;
  for (let i = 0; i < segs.length; i++) {
    for (let j = i + 1; j < segs.length; j++) {
      if (segmentsCross(segs[i].a, segs[i].b, segs[j].a, segs[j].b)) n++;
    }
  }
  return n;
}

/** 座標マップの外接矩形の縦横比。 */
function aspect(positions: PositionMap): number {
  const xs = Object.values(positions).map((p) => p.x);
  const ys = Object.values(positions).map((p) => p.y);
  const w = Math.max(...xs) - Math.min(...xs) + NODE_W;
  const h = Math.max(...ys) - Math.min(...ys) + NODE_H;
  return w / h;
}

describe("computeLayout", () => {
  it("空の盤面では空のマップを返す", async () => {
    expect(await computeLayout([], [])).toEqual({});
  });

  it("決定的である（同じ入力から同じ座標）", async () => {
    const { nodes, edges } = chain(24);
    const a = await computeLayout(nodes, edges);
    const b = await computeLayout(nodes, edges);
    expect(a).toEqual(b);
  });

  it("一本鎖24マスを複数行に折り返す", async () => {
    const { nodes, edges } = chain(24);
    const positions = await computeLayout(nodes, edges);
    const rows = new Set(Object.values(positions).map((p) => Math.round(p.y)));
    expect(rows.size).toBeGreaterThan(1);
  });

  it("一本鎖24マスの縦横比を 3.0 以下に収める", async () => {
    const { nodes, edges } = chain(24);
    expect(aspect(await computeLayout(nodes, edges))).toBeLessThanOrEqual(3.0);
  });

  it("一本鎖24マスでは交差が発生しない", async () => {
    const { nodes, edges } = chain(24);
    expect(countCrossings(await computeLayout(nodes, edges), edges)).toBe(0);
  });

  it("一本鎖では行ごとに進行方向が反転する（蛇行）", async () => {
    const { nodes, edges } = chain(24);
    const positions = await computeLayout(nodes, edges);
    // y でグループ化し、各行を x 昇順に並べたとき、
    // 偶数行は添字が昇順、奇数行は降順になる。
    const byRow = new Map<number, string[]>();
    for (const [id, p] of Object.entries(positions)) {
      const k = Math.round(p.y);
      if (!byRow.has(k)) byRow.set(k, []);
      byRow.get(k)!.push(id);
    }
    const rows = [...byRow.keys()].sort((a, b) => a - b);
    expect(rows.length).toBeGreaterThan(1);
    rows.forEach((rowY, r) => {
      const idx = byRow
        .get(rowY)!
        .sort((a, b) => positions[a].x - positions[b].x)
        .map((id) => Number(id.slice(1)));
      const ascending = idx.every((v, i) => i === 0 || v > idx[i - 1]);
      const descending = idx.every((v, i) => i === 0 || v < idx[i - 1]);
      expect(r % 2 === 0 ? ascending : descending).toBe(true);
    });
  });
});

describe("arrangeRows", () => {
  /** 層番号と層内位置から PlacedNode を作る（ELK 左上基準）。 */
  function layer(ids: string[], layerIndex: number): PlacedNode[] {
    return ids.map((id, i) => ({
      id,
      x: layerIndex * 230,
      y: i * 100,
      width: NODE_W,
      height: NODE_H,
    }));
  }

  it("蛇行ありは同方向折り返しより交差が増えない", () => {
    // 6層の鎖を3層/行で2行に折り返す。
    const layers = [
      layer(["c0"], 0), layer(["c1"], 1), layer(["c2"], 2),
      layer(["c3"], 3), layer(["c4"], 4), layer(["c5"], 5),
    ];
    const edges: LayoutEdge[] = [
      { from: "c0", to: "c1" }, { from: "c1", to: "c2" }, { from: "c2", to: "c3" },
      { from: "c3", to: "c4" }, { from: "c4", to: "c5" },
    ];
    const naive = countCrossings(arrangeRows(layers, 3, false).positions, edges);
    const snake = countCrossings(arrangeRows(layers, 3, true).positions, edges);
    expect(snake).toBeLessThanOrEqual(naive);
  });

  it("中心座標を返す（左上ではない）", () => {
    const layers = [layer(["c0"], 0)];
    const { positions } = arrangeRows(layers, 1, true);
    // 行内で正規化され左上は (0,0)。中心はその半分ずれる。
    expect(positions.c0).toEqual({ x: NODE_W / 2, y: NODE_H / 2 });
  });
});

describe("groupIntoLayers", () => {
  it("同じ x のノードを1つの層にまとめ、x 昇順で返す", () => {
    const placed: PlacedNode[] = [
      { id: "b", x: 230, y: 0, width: NODE_W, height: NODE_H },
      { id: "a1", x: 0, y: 0, width: NODE_W, height: NODE_H },
      { id: "a2", x: 0, y: 100, width: NODE_W, height: NODE_H },
    ];
    const layers = groupIntoLayers(placed);
    expect(layers.map((l) => l.map((n) => n.id))).toEqual([["a1", "a2"], ["b"]]);
  });
});
```

- [ ] **Step 3: テストが失敗することを確認する**

Run: `cd sugo-gui && npx vitest run src/lib/layout.test.ts`
Expected: FAIL。`./layout` が解決できない。

- [ ] **Step 4: layout.ts を実装する**

`sugo-gui/src/lib/layout.ts` を作成する。

```ts
// 蛇行（ジグザグ）レイアウト。
//
// ELK には層への割り当てとノード順序の決定（交差最小化）だけを任せ、
// 行への折り返しは自前で行う。ELK 自身の wrapping 機能を使わない理由は、
// 出力に層の情報が含まれず、分岐のあるグラフでは座標から行の境界を
// 復元できないため。行が確定できなければ「奇数行を反転する」蛇行処理が
// 成立しない。設計の詳細は
// docs/superpowers/specs/2026-09-22-cell-layout-design.md を参照。
//
// このモジュールは cytoscape・Tauri・DB のいずれにも依存しない純粋な計算で、
// 同じ入力からは常に同じ座標を返す。
import ELK from "elkjs/lib/elk.bundled.js";
import type { PositionMap } from "./positions";

export type LayoutNode = { id: string; width: number; height: number };
export type LayoutEdge = { from: string; to: string };
/** ELK 由来の配置結果。x/y はノードの左上。 */
export type PlacedNode = { id: string; x: number; y: number; width: number; height: number };
/** positions はセル中心の座標。width/height は外接矩形の大きさ。 */
export type ArrangeResult = { positions: PositionMap; width: number; height: number };

/** 目標の縦横比。横長すぎると文字が潰れ、縦長すぎると全体が見渡せない。 */
const TARGET_ASPECT = 1.6;
/** 行と行の縦の余白。 */
const ROW_GAP = 90;

const elk = new ELK();

/** ELK layered を折り返しなしで実行し、層割り当てとノード順序を得る。 */
async function runElk(nodes: LayoutNode[], edges: LayoutEdge[]): Promise<PlacedNode[]> {
  const graph = await elk.layout({
    id: "root",
    layoutOptions: {
      "elk.algorithm": "layered",
      "elk.direction": "RIGHT",
      // 折り返しは自前で行うため OFF。理由はモジュール冒頭のコメント参照。
      "elk.layered.wrapping.strategy": "OFF",
      "elk.spacing.nodeNode": "40",
      "elk.layered.spacing.nodeNodeBetweenLayers": "80",
    },
    children: nodes.map((n) => ({ id: n.id, width: n.width, height: n.height })),
    edges: edges.map((e, i) => ({ id: `e${i}`, sources: [e.from], targets: [e.to] })),
  });
  return (graph.children ?? []).map((c: Record<string, number | string>) => ({
    id: c.id as string,
    x: c.x as number,
    y: c.y as number,
    width: c.width as number,
    height: c.height as number,
  }));
}

/**
 * 同じ x のノードを1つの層とみなし、x 昇順の層配列を返す。
 *
 * direction=RIGHT の layered 出力では同一層のノードが同一の x に揃うため、
 * x が層の識別子として使える。
 */
export function groupIntoLayers(placed: PlacedNode[]): PlacedNode[][] {
  const byX = new Map<number, PlacedNode[]>();
  for (const n of placed) {
    const k = Math.round(n.x);
    if (!byX.has(k)) byX.set(k, []);
    byX.get(k)!.push(n);
  }
  return [...byX.keys()]
    .sort((a, b) => a - b)
    .map((k) => byX.get(k)!.slice().sort((a, b) => a.y - b.y));
}

/**
 * 層配列を `layersPerRow` 層ずつの行に切り、行を縦に積む。
 *
 * `serpentine` が true のとき、奇数行は行内で x を左右反転する。これにより
 * 行の進行方向が交互になり、行末から次の行頭へのエッジが画面を横断する
 * 長い線ではなく真下への短い線になる。
 */
export function arrangeRows(
  layers: PlacedNode[][],
  layersPerRow: number,
  serpentine = true,
): ArrangeResult {
  const positions: PositionMap = {};
  let rowTop = 0;
  let maxWidth = 0;

  for (let r = 0; r * layersPerRow < layers.length; r++) {
    const flat = layers.slice(r * layersPerRow, (r + 1) * layersPerRow).flat();
    if (flat.length === 0) continue;

    const minX = Math.min(...flat.map((n) => n.x));
    const rowWidth = Math.max(...flat.map((n) => n.x + n.width)) - minX;
    const minY = Math.min(...flat.map((n) => n.y));
    const rowHeight = Math.max(...flat.map((n) => n.y + n.height)) - minY;

    for (const n of flat) {
      const localX = n.x - minX;
      const left = serpentine && r % 2 === 1 ? rowWidth - (localX + n.width) : localX;
      // 左上基準から中心基準へ変換する（cytoscape の node.position() に合わせる）
      positions[n.id] = {
        x: left + n.width / 2,
        y: rowTop + (n.y - minY) + n.height / 2,
      };
    }

    maxWidth = Math.max(maxWidth, rowWidth);
    rowTop += rowHeight + ROW_GAP;
  }

  return { positions, width: maxWidth, height: Math.max(0, rowTop - ROW_GAP) };
}

/** 縦横比が目標に最も近くなる「1行あたりの層数」を全探索で選ぶ。 */
export function chooseLayersPerRow(
  layers: PlacedNode[][],
  targetAspect = TARGET_ASPECT,
): number {
  let best = Math.max(1, layers.length);
  let bestDiff = Number.POSITIVE_INFINITY;
  for (let k = 1; k <= layers.length; k++) {
    const { width, height } = arrangeRows(layers, k);
    if (height === 0) continue;
    const diff = Math.abs(width / height - targetAspect);
    if (diff < bestDiff) {
      bestDiff = diff;
      best = k;
    }
  }
  return best;
}

/** 盤面から蛇行レイアウトの座標（セル中心）を計算する。 */
export async function computeLayout(
  nodes: LayoutNode[],
  edges: LayoutEdge[],
  targetAspect = TARGET_ASPECT,
): Promise<PositionMap> {
  if (nodes.length === 0) return {};
  const placed = await runElk(nodes, edges);
  const layers = groupIntoLayers(placed);
  return arrangeRows(layers, chooseLayersPerRow(layers, targetAspect)).positions;
}
```

- [ ] **Step 5: テストが通ることを確認する**

Run: `cd sugo-gui && npx vitest run src/lib/layout.test.ts`
Expected: 9件 PASS

蛇行の検証（`一本鎖では行ごとに進行方向が反転する`）や交差数の検証が落ちる場合は、
実装ではなく期待値の立て方を疑う前に、まず `arrangeRows` の反転条件
（`r % 2 === 1` と `rowWidth - (localX + n.width)`）を確認すること。
縦横比の閾値 3.0 が厳しすぎて落ちる場合は、`TARGET_ASPECT` ではなくテストの閾値を
実測値に合わせて緩める。ただし 24 マスで 5.0 を超えるなら実装に問題がある。

- [ ] **Step 6: 型チェックを通す**

Run: `cd sugo-gui && npx vue-tsc --noEmit`
Expected: エラーなし

`elkjs` が型定義を同梱していない場合は、`sugo-gui/src/types/` に
既存の `cytoscape-edgehandles.d.ts` と同じ流儀で最小限の宣言を追加する。

- [ ] **Step 7: コミット**

```bash
git add sugo-gui/package.json sugo-gui/package-lock.json \
        sugo-gui/src/lib/layout.ts sugo-gui/src/lib/layout.test.ts
git commit -m "feat: ELK を用いた蛇行レイアウトエンジンを追加する"
```

---

### Task 6: BoardGraph を蛇行レイアウトに繋ぎ替える

**Files:**
- Modify: `sugo-gui/src/components/BoardGraph.vue`（dagre 関連の削除と `computeLayout` 呼び出し）
- Modify: `sugo-gui/src/components/BoardGraph.test.ts`
- Modify: `sugo-gui/package.json`（dagre 依存の削除）

**Interfaces:**
- Consumes: `computeLayout`, `LayoutNode`, `LayoutEdge`（Task 5）／ `loadPositions`, `savePositions`（Task 4）
- Produces: `BoardGraph.vue` に `async function relayoutAll(): Promise<void>`（全体を組み直して保存する）。Task 7 の「整列」ボタンが使う。

- [ ] **Step 1: 失敗するテストを書く**

`sugo-gui/src/components/BoardGraph.test.ts` に追加する。
既存のモック（cytoscape / cytoscape-dagre / cytoscape-edgehandles / ../lib/positions）はそのまま使い、
`../lib/layout` のモックを足す。

```ts
const computeLayoutMock = vi.fn(async () => ({
  c1: { x: 10, y: 20 },
  c2: { x: 30, y: 20 },
}));
vi.mock("../lib/layout", () => ({
  computeLayout: (...a: unknown[]) => computeLayoutMock(...a),
}));
```

```ts
it("座標が未保存のとき computeLayout の結果を各ノードへ適用する", async () => {
  // loadPositions は既定で {} を返すモックなので、全ノードが未配置になる。
  const wrapper = mountBoardGraph({
    cells: [
      { id: "c1", name: "一", status: "active", terminal: false },
      { id: "c2", name: "二", status: "active", terminal: true },
    ],
    edges: [{ from: "c1", to: "c2", label: "next", guard: null }],
  });
  await flushPromises();

  expect(computeLayoutMock).toHaveBeenCalled();
  const cy = lastCy();
  expect(cy.getElementById("c1").position).toHaveBeenCalledWith({ x: 10, y: 20 });
});

it("computeLayout へ渡すノードに幅と高さが含まれる", async () => {
  mountBoardGraph({
    cells: [{ id: "c1", name: "一", status: "active", terminal: true }],
    edges: [],
  });
  await flushPromises();

  const [nodes] = computeLayoutMock.mock.calls[0] as [Array<{ id: string; width: number; height: number }>];
  expect(nodes[0].id).toBe("c1");
  expect(nodes[0].width).toBe(150);
  expect(nodes[0].height).toBeGreaterThan(0);
});
```

`mountBoardGraph` / `lastCy` は既存テストのヘルパー名に合わせること
（現在は `(cytoscape as any).__lastCy` を返す関数が定義されている）。
`flushPromises` は `@vue/test-utils` から import する。

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd sugo-gui && npx vitest run src/components/BoardGraph.test.ts`
Expected: FAIL。`computeLayout` が呼ばれない（まだ dagre を使っている）。

- [ ] **Step 3: BoardGraph.vue を繋ぎ替える**

1. dagre の import と登録を削除する。

```ts
// 削除する行（現 59, 60, 64 行目付近）
import cytoscapeDagre from "cytoscape-dagre";
cytoscape.use(cytoscapeDagre as cytoscape.Ext);
```

2. `DAGRE_OPTIONS` 定数（現 258-265 行目）を削除する。

3. import を追加する。

```ts
import { computeLayout, type LayoutNode, type LayoutEdge } from "../lib/layout";
```

4. ノード寸法を組み立てる関数を足す。幅はスタイルの `width: "150px"` と揃える。

```ts
// STYLES の node.width と同じ値。レイアウト計算はスタイルを読めないため
// ここで明示する。片方を変えたらもう片方も変えること。
const NODE_WIDTH = 150;

function buildLayoutInput(): { nodes: LayoutNode[]; edges: LayoutEdge[] } {
  const nodes = props.cells.map((c) => ({
    id: c.id,
    width: NODE_WIDTH,
    height: buildHeight(buildLabel(c)),
  }));
  const edges = props.edges.map((e) => ({ from: e.from, to: e.to }));
  return { nodes, edges };
}
```

5. 全体整列の関数を足す。Task 7 のボタンからも使う。

```ts
/** 全体を蛇行レイアウトで組み直し、結果を保存する。 */
async function relayoutAll() {
  if (!cy) return;
  const { nodes, edges } = buildLayoutInput();
  const positions = await computeLayout(nodes, edges);
  if (!cy) return; // await をまたぐ間に破棄された可能性がある
  applyPositions(positions);
  await saveCurrentPositions();
  fitView();
  computeMarkerPositions();
}

defineExpose({ relayoutAll });
```

6. `placeNodes` の else 側を差し替える。

```ts
async function placeNodes() {
  if (!cy) return;
  const saved = await loadPositions(props.harnessId);
  if (!cy) return;

  const missingIds = props.cells.filter((c) => saved[c.id] == null).map((c) => c.id);
  const hasAnySaved = Object.keys(saved).length > 0;

  if (hasAnySaved && missingIds.length < props.cells.length) {
    // 既存の配置がある: それを保持し、未配置のセルだけ追加配置する
    applyPositions(saved);
    if (missingIds.length > 0) {
      placeNewNodes(saved, missingIds);
      await saveCurrentPositions();
    }
    fitView();
    computeMarkerPositions();
  } else {
    // 保存が全く無い（新規ハーネス）→ 蛇行レイアウトで全体を配置する
    await relayoutAll();
  }
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd sugo-gui && npm test`
Expected: 全件 PASS

`cytoscape-dagre` のモックが不要になるので、既存の `vi.mock("cytoscape-dagre", ...)` は削除する。

- [ ] **Step 5: dagre 依存を外す**

Run: `cd sugo-gui && npm uninstall cytoscape-dagre @dagrejs/dagre`

Run: `cd sugo-gui && grep -rn "dagre" src/ && echo "残存あり" || echo "残存なし"`
Expected: 「残存なし」

- [ ] **Step 6: 実機で確認する**

Run: `cd sugo-gui && npm run tauri dev`

確認内容: マスを20個以上持つハーネスを新規に作る（または既存ハーネスの
`cell_positions` を `sqlite3 <db_path> "DELETE FROM cell_positions WHERE harness_id='<id>';"` で消す）。
開いたときに横一列ではなく複数行に折り返され、行ごとに進行方向が反転していること。
セル名の文字が読める大きさであること。

- [ ] **Step 7: コミット**

```bash
git add sugo-gui/src/components/BoardGraph.vue sugo-gui/src/components/BoardGraph.test.ts \
        sugo-gui/package.json sugo-gui/package-lock.json
git commit -m "feat: 盤面の自動配置を dagre 横一列から蛇行レイアウトへ置き換える"
```

---

### Task 7: 「整列」ボタン

**Files:**
- Modify: `sugo-gui/src/views/HarnessView.vue`
- Modify: `sugo-gui/src/views/HarnessView.test.ts`

**Interfaces:**
- Consumes: `BoardGraph` の `relayoutAll()`（Task 6 で `defineExpose` 済み）
- Produces: なし

- [ ] **Step 1: 失敗するテストを書く**

`sugo-gui/src/views/HarnessView.test.ts` の既存の BoardGraph モック（現 21-23 行目）を差し替える。
現在は `template: "<div/>"` のスタブだが、`ref="boardGraph"` から `relayoutAll` を掴めるよう
`expose` する形にする。`<script setup>` のコンポーネントは外から ref を代入できないため、
スタブ側で公開するのが唯一の手になる。

```ts
import { h } from "vue";

const relayoutAllMock = vi.fn(async () => {});
vi.mock("../components/BoardGraph.vue", () => ({
  default: {
    name: "BoardGraph",
    emits: ["select", "edge-edit", "edge-delete", "node-delete", "node-rename", "connect"],
    setup(_props: unknown, { expose }: { expose: (e: Record<string, unknown>) => void }) {
      expose({ relayoutAll: relayoutAllMock });
      return () => h("div");
    },
  },
}));
```

emits の一覧は `BoardGraph.vue` の `defineEmits` と揃えること。既存モックが
`["select", "edge-edit"]` しか宣言していなくても動いていたのは、既存テストが
その2つしか使っていなかったためで、減らす方向の変更はしない。

テスト本体を追加する。マウントの手順は同ファイルの既存テストに倣うこと。

```ts
it("整列ボタンを押すと BoardGraph の relayoutAll を呼ぶ", async () => {
  relayoutAllMock.mockClear();
  const wrapper = mountHarnessView();
  await flushPromises();

  await wrapper.find('[data-testid="relayout"]').trigger("click");
  await flushPromises();

  expect(relayoutAllMock).toHaveBeenCalled();
});
```

`mountHarnessView` は既存テストがマウントに使っている手続きの名前に読み替えること
（同ファイルに共通ヘルパーが無ければ、既存テストのマウント部分をそのまま書く）。

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd sugo-gui && npx vitest run src/views/HarnessView.test.ts`
Expected: FAIL。`[data-testid="relayout"]` が見つからない。

- [ ] **Step 3: ボタンを足す**

`sugo-gui/src/views/HarnessView.vue` の「+ マスを追加」ボタン（現 9-12 行目）を
横並びのコンテナで包み、その中に「整列」を追加する。主操作である「+ マスを追加」を
青のまま残し、「整列」は副次操作なのでグレー系にする。

```vue
      <div class="flex items-center gap-2">
        <button
          data-testid="relayout"
          class="bg-gray-100 text-gray-700 px-4 py-2 rounded hover:bg-gray-200"
          @click="onRelayout"
        >整列</button>
        <button
          class="bg-blue-500 text-white px-4 py-2 rounded hover:bg-blue-600"
          @click="showAddCell = true"
        >+ マスを追加</button>
      </div>
```

`<script setup>` に追加する。

```ts
const boardGraph = ref<{ relayoutAll: () => Promise<void> } | null>(null);

/** 盤面を蛇行レイアウトで組み直す。手で動かした配置は失われる。 */
async function onRelayout() {
  await boardGraph.value?.relayoutAll();
}
```

`<BoardGraph ... />` に `ref="boardGraph"` を付ける。

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd sugo-gui && npm test`
Expected: 全件 PASS

- [ ] **Step 5: 実機で確認する**

Run: `cd sugo-gui && npm run tauri dev`

確認内容: ノードを適当にドラッグして配置を崩したあと「整列」を押す。
蛇行レイアウトに戻り、一覧へ戻って開き直しても整列後の位置が保たれていること。

- [ ] **Step 6: コミット**

```bash
git add sugo-gui/src/views/HarnessView.vue sugo-gui/src/views/HarnessView.test.ts
git commit -m "feat: 盤面を組み直す「整列」ボタンを追加する"
```

---

### Task 8: MCP ツール3本

**Files:**
- Modify: `sugo-mcp/src/tools.rs`
- Modify: `sugo-mcp/src/main.rs:20-63`（struct と `new`）
- Modify: `sugo-mcp/src/main.rs`（ツール本体を `sugo_get_cell` の後ろに追加）
- Modify: `sugo-mcp/src/main.rs:1042`（server instructions のツール一覧）
- Modify: `sugo-mcp/src/main.rs:1060-1075`（`main` の組み立て）

**Interfaces:**
- Consumes: `CellPositionRepository`, `SqliteCellPositionRepository`（Task 1）
- Produces: MCP ツール `sugo_get_layout` / `sugo_set_layout` / `sugo_relayout`

- [ ] **Step 1: 引数型を足す**

`sugo-mcp/src/tools.rs` に追加する。

```rust
/// Arguments for `sugo_get_layout`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetLayoutArgs {
    /// Target harness id.
    pub harness_id: String,
}

/// One cell's display position. x/y are the cell's center coordinates.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct PositionArg {
    /// Cell id. Must exist in the harness's current board version.
    pub cell_id: String,
    /// Center x coordinate.
    pub x: f64,
    /// Center y coordinate.
    pub y: f64,
}

/// Arguments for `sugo_set_layout`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetLayoutArgs {
    /// Target harness id.
    pub harness_id: String,
    /// Positions to overwrite. Cells not listed keep their current position.
    pub positions: Vec<PositionArg>,
}

/// Arguments for `sugo_relayout`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RelayoutArgs {
    /// Target harness id.
    pub harness_id: String,
}
```

同ファイルの `#[cfg(test)] mod tests` に round-trip テストを追加する。既存の
`edit_args_round_trip` / `advance_args_missing_step_token_errors` の書式に倣うこと。

```rust
#[test]
fn set_layout_args_round_trip() {
    let v = serde_json::json!({
        "harness_id": "h1",
        "positions": [{ "cell_id": "c1", "x": 1.5, "y": 2.5 }]
    });
    let args: SetLayoutArgs = serde_json::from_value(v).expect("deserialize ok");
    assert_eq!(args.harness_id, "h1");
    assert_eq!(args.positions.len(), 1);
    assert_eq!(args.positions[0].cell_id, "c1");
    assert_eq!(args.positions[0].x, 1.5);
    assert_eq!(args.positions[0].y, 2.5);
}

#[test]
fn set_layout_args_missing_positions_errors() {
    let v = serde_json::json!({ "harness_id": "h1" });
    serde_json::from_value::<SetLayoutArgs>(v).expect_err("positions is required");
}

#[test]
fn get_layout_args_round_trip() {
    let v = serde_json::json!({ "harness_id": "h1" });
    let args: GetLayoutArgs = serde_json::from_value(v).expect("deserialize ok");
    assert_eq!(args.harness_id, "h1");
}

#[test]
fn relayout_args_round_trip() {
    let v = serde_json::json!({ "harness_id": "h1" });
    let args: RelayoutArgs = serde_json::from_value(v).expect("deserialize ok");
    assert_eq!(args.harness_id, "h1");
}
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cargo test -p sugo-mcp set_layout_args`
Expected: コンパイルエラー。`SetLayoutArgs` が存在しない。

- [ ] **Step 3: 型テストを通す**

Run: `cargo test -p sugo-mcp`
Expected: Step 1 で追加した4件を含め全件 PASS

- [ ] **Step 4: サーバに座標リポジトリを持たせる**

`sugo-mcp/src/main.rs` の `SugoServer` struct（現 28 行目付近）に追加する。

```rust
    pos_repo: Arc<SqliteCellPositionRepository>,
```

`SugoServer::new` の引数と本体（現 47-63 行目）に `pos_repo` を通す。
`use sugo_infra::sqlite::cell_position_repository::SqliteCellPositionRepository;` を追加する。

`main`（現 1073 行目付近、`run_repo` を作った直後）に追加する。

```rust
    // 座標用にもう1本コネクションを開く。run_repo と同様、スキーマ適用は
    // SqliteHarnessRepository::open が済ませている。
    let pos_conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| anyhow::anyhow!("open pos_repo DB: {e}"))?;
    let pos_repo = Arc::new(SqliteCellPositionRepository::new(std::sync::Mutex::new(pos_conn)));
```

`SugoServer::new(...)` の呼び出し箇所すべてに `pos_repo.clone()` を渡す。
テスト内にも `SugoServer::new` を呼ぶヘルパーがあるため、`cargo test -p sugo-mcp` で
コンパイルエラーが出る箇所をすべて直すこと。

- [ ] **Step 5: ツール3本を実装する**

`sugo-mcp/src/main.rs` の `sugo_get_cell`（現 301 行目で終わる）の直後に追加する。

```rust
    /// Read the display positions of a harness's cells.
    #[tool(
        description = "Get a harness's cell display layout: { harness_id, cells:[{cell_id, \
        name, x, y}], edges:[{from,to,label}] }. x/y are the cell's CENTER coordinates and \
        are null for cells that have no saved position (those are auto-placed by the GUI on \
        next render). Use this before sugo_set_layout so a repositioning is grounded in the \
        actual current layout."
    )]
    async fn sugo_get_layout(
        &self,
        Parameters(args): Parameters<tools::GetLayoutArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        use sugo_core::ports::cell_position_repository::CellPositionRepository;
        use sugo_core::usecase::get_status::get_status;

        let st = get_status(self.repo.as_ref(), &args.harness_id)
            .await
            .map_err(error::to_tool_error)?;
        let saved = self
            .pos_repo
            .list(&args.harness_id)
            .await
            .map_err(error::to_tool_error)?;

        let by_id: std::collections::HashMap<&str, &sugo_core::domain::cell_position::CellPosition> =
            saved.iter().map(|p| (p.cell_id.as_str(), p)).collect();

        let cells: Vec<serde_json::Value> = st
            .definition
            .cells
            .iter()
            .map(|c| {
                let p = by_id.get(c.id.as_str());
                serde_json::json!({
                    "cell_id": c.id,
                    "name": c.name,
                    "x": p.map(|p| p.x),
                    "y": p.map(|p| p.y),
                })
            })
            .collect();

        let edges: Vec<serde_json::Value> = st
            .definition
            .edges
            .iter()
            .map(|e| serde_json::json!({ "from": e.from, "to": e.to, "label": e.label }))
            .collect();

        let payload = serde_json::json!({
            "harness_id": args.harness_id,
            "cells": cells,
            "edges": edges,
        });
        Ok(CallToolResult::success(vec![Content::text(
            payload.to_string(),
        )]))
    }

    /// Overwrite the display positions of specific cells.
    #[tool(
        description = "Set the display position of specific cells: { harness_id, \
        positions:[{cell_id, x, y}] }. x/y are CENTER coordinates. Cells not listed keep \
        their current position. This does NOT create a new board version and does not take \
        the optimistic lock — layout is display-only data. Errors if any cell_id is absent \
        from the harness's current board version."
    )]
    async fn sugo_set_layout(
        &self,
        Parameters(args): Parameters<tools::SetLayoutArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        use sugo_core::domain::cell_position::CellPosition;
        use sugo_core::ports::cell_position_repository::CellPositionRepository;
        use sugo_core::usecase::get_status::get_status;

        let st = get_status(self.repo.as_ref(), &args.harness_id)
            .await
            .map_err(error::to_tool_error)?;
        let known: std::collections::HashSet<&str> =
            st.definition.cells.iter().map(|c| c.id.as_str()).collect();

        // 盤面に無いセルの座標を書くと、GUI 側の全置換保存で黙って消える上に
        // 呼び出し側は成功したと誤解する。先に弾く。
        for p in &args.positions {
            if !known.contains(p.cell_id.as_str()) {
                return Err(error::to_tool_error(
                    sugo_core::error::CoreError::NotFound(p.cell_id.clone()),
                ));
            }
        }

        let mapped: Vec<CellPosition> = args
            .positions
            .iter()
            .map(|p| CellPosition { cell_id: p.cell_id.clone(), x: p.x, y: p.y })
            .collect();
        self.pos_repo
            .upsert(&args.harness_id, &mapped)
            .await
            .map_err(error::to_tool_error)?;

        let payload = serde_json::json!({
            "harness_id": args.harness_id,
            "updated": mapped.len(),
        });
        Ok(CallToolResult::success(vec![Content::text(
            payload.to_string(),
        )]))
    }

    /// Discard a harness's saved layout so the GUI re-computes it.
    #[tool(
        description = "Clear a harness's saved cell positions. The GUI re-computes a fresh \
        serpentine layout for every cell on its next render. Use this when the layout is a \
        mess and a clean rebuild is wanted; any manual positioning is lost. Returns \
        { harness_id, cleared: true }."
    )]
    async fn sugo_relayout(
        &self,
        Parameters(args): Parameters<tools::RelayoutArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        use sugo_core::ports::cell_position_repository::CellPositionRepository;
        use sugo_core::usecase::get_status::get_status;

        // 存在しないハーネス id を黙って成功させないよう、先に引く。
        get_status(self.repo.as_ref(), &args.harness_id)
            .await
            .map_err(error::to_tool_error)?;

        self.pos_repo
            .clear(&args.harness_id)
            .await
            .map_err(error::to_tool_error)?;

        let payload = serde_json::json!({
            "harness_id": args.harness_id,
            "cleared": true,
        });
        Ok(CallToolResult::success(vec![Content::text(
            payload.to_string(),
        )]))
    }
```

`st.definition.edges` の要素が `from` / `to` / `label` というフィールド名かどうかは
`sugo_core::domain::edge::Edge` の定義で確認し、違えば合わせること。

- [ ] **Step 6: server instructions を更新する**

`sugo-mcp/src/main.rs:1042` のツール一覧文字列に3本を追加する。
`sugo_move_harness` の後ろに `, sugo_get_layout, sugo_set_layout, sugo_relayout` を足し、
末尾に一文を加える。

```
Cell display positions live outside the board definition: sugo_get_layout / \
sugo_set_layout / sugo_relayout never create a board version.
```

- [ ] **Step 7: 統合テストを書く**

`sugo-mcp/src/main.rs` の `#[cfg(test)] mod tests` に追加する。
既存テストが `SugoServer` をどう組み立てているかを確認し、同じヘルパーを使うこと。

```rust
#[tokio::test]
async fn set_layout_rejects_a_cell_absent_from_the_board() {
    let (server, harness_id) = server_with_one_cell_harness().await;

    let err = server
        .sugo_set_layout(Parameters(tools::SetLayoutArgs {
            harness_id: harness_id.clone(),
            positions: vec![tools::PositionArg { cell_id: "nope".into(), x: 0.0, y: 0.0 }],
        }))
        .await
        .expect_err("unknown cell must be rejected");
    assert!(format!("{err:?}").contains("nope"));
}

#[tokio::test]
async fn set_layout_then_get_layout_round_trips() {
    let (server, harness_id) = server_with_one_cell_harness().await;

    server
        .sugo_set_layout(Parameters(tools::SetLayoutArgs {
            harness_id: harness_id.clone(),
            positions: vec![tools::PositionArg { cell_id: "c1".into(), x: 12.0, y: 34.0 }],
        }))
        .await
        .expect("set ok");

    let res = server
        .sugo_get_layout(Parameters(tools::GetLayoutArgs { harness_id: harness_id.clone() }))
        .await
        .expect("get ok");
    let text = first_text(&res);
    let v: serde_json::Value = serde_json::from_str(&text).expect("json");
    assert_eq!(v["cells"][0]["cell_id"], "c1");
    assert_eq!(v["cells"][0]["x"], 12.0);
    assert_eq!(v["cells"][0]["y"], 34.0);
}

#[tokio::test]
async fn relayout_clears_saved_positions() {
    let (server, harness_id) = server_with_one_cell_harness().await;

    server
        .sugo_set_layout(Parameters(tools::SetLayoutArgs {
            harness_id: harness_id.clone(),
            positions: vec![tools::PositionArg { cell_id: "c1".into(), x: 12.0, y: 34.0 }],
        }))
        .await
        .expect("set ok");
    server
        .sugo_relayout(Parameters(tools::RelayoutArgs { harness_id: harness_id.clone() }))
        .await
        .expect("relayout ok");

    let res = server
        .sugo_get_layout(Parameters(tools::GetLayoutArgs { harness_id }))
        .await
        .expect("get ok");
    let v: serde_json::Value = serde_json::from_str(&first_text(&res)).expect("json");
    assert!(v["cells"][0]["x"].is_null());
}
```

`server_with_one_cell_harness` と `first_text` は既存テストに同等のヘルパーがあればそれを使い、
無ければ既存テストの組み立て部分を切り出して作る。セル id は既定テンプレートの実際の値に合わせること
（`sugo_create_harness` を definition 無しで呼んだときのセル id を `sugo_status` で確認する）。

- [ ] **Step 8: テストが通ることを確認する**

Run: `cargo test -p sugo-mcp`
Expected: 全件 PASS

- [ ] **Step 9: ワークスペース全体を確認する**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: 全件 PASS、警告なし

- [ ] **Step 10: コミット**

```bash
git add sugo-mcp/src/tools.rs sugo-mcp/src/main.rs
git commit -m "feat: セル座標を読み書き・再整列する MCP ツールを追加する"
```

---

### Task 9: E2E と仕上げ

**Files:**
- Create: `sugo-gui/e2e/layout.spec.ts`
- Modify: `CLAUDE.md`

**Interfaces:**
- Consumes: Task 1-8 のすべて
- Produces: なし

- [ ] **Step 1: E2E を書く**

`sugo-gui/e2e/layout.spec.ts` を作成する。既存 `sugo-gui/e2e/folders.spec.ts` の
セットアップ（アプリの起動、ハーネス作成の手順、セレクタの流儀）をそのまま踏襲すること。

```ts
import { test, expect } from "@playwright/test";

test("新規ハーネスの盤面は横一列にならない", async ({ page }) => {
  // folders.spec.ts と同じ手順でアプリを開き、ハーネスを新規作成し、
  // マスを6個追加して一本鎖に繋ぐ。
  await setupHarnessWithChain(page, 6);

  // cytoscape のノード座標を読み出す。
  const ys = await page.evaluate(() => {
    const cy = (window as unknown as { __sugoCy?: { nodes: () => { map: (f: (n: unknown) => number) => number[] } } }).__sugoCy;
    if (!cy) throw new Error("cy handle not exposed");
    return cy.nodes().map((n: { position: () => { y: number } }) => Math.round(n.position().y));
  });

  // 折り返されていれば y が複数種類になる。横一列なら1種類しかない。
  expect(new Set(ys).size).toBeGreaterThan(1);
});
```

`window.__sugoCy` は本番コードに存在しない。`BoardGraph.vue` の `initCy()` の末尾で
開発・テストビルドのときだけ公開する。

```ts
  // E2E から盤面の実座標を検証するためのハンドル。本番ビルドでは公開しない。
  if (import.meta.env.DEV) {
    (window as unknown as { __sugoCy?: cytoscape.Core }).__sugoCy = cy;
  }
```

`setupHarnessWithChain` は `folders.spec.ts` のハーネス作成部分を流用して書く。
E2E がこの用途のために本番コードへフックを足すのが受け入れられない場合は、
代替として「セル名のテキストが読める大きさで描画されている」ことを
スクリーンショット比較で検証する形に切り替えてよい。その判断は実装者に委ねる。

- [ ] **Step 2: E2E が通ることを確認する**

Run: `cd sugo-gui && npm run test:e2e -- layout.spec.ts`
Expected: PASS

- [ ] **Step 3: CLAUDE.md にデータ方針を追記する**

`CLAUDE.md` の「データ方針」節に一行足す。

```markdown
- **セルの表示座標は `cell_positions` テーブル**に持つ。盤面定義（`board_versions`）には入れない。座標を変えても board version は上がらない
```

- [ ] **Step 4: 全体を通しで確認する**

Run: `cargo test --workspace && cd sugo-gui && npm test && npx vue-tsc --noEmit`
Expected: すべて PASS

- [ ] **Step 5: 実機で最終確認する**

Run: `cd sugo-gui && npm run tauri dev`

確認内容を順に行う。

1. 20マス以上のハーネスを新規作成 → 蛇行レイアウトで表示される
2. マスを1つ追加 → 既存の配置が動かず、新しいマスだけ追加される
3. ノードをドラッグ → 開き直しても位置が保たれる
4. 「整列」を押す → 蛇行レイアウトに戻る
5. 別セッションの Claude から `sugo_get_layout` → 座標が読める
6. `sugo_relayout` を呼んでから GUI を開き直す → 組み直されている

- [ ] **Step 6: コミット**

```bash
git add sugo-gui/e2e/layout.spec.ts sugo-gui/src/components/BoardGraph.vue CLAUDE.md
git commit -m "test: 新規ハーネスが横一列にならないことを E2E で検証する"
```

---

## 積み残し

以下は本計画の対象外とする。必要になった時点で別途扱う。

- **エッジのルーティング改善**: 現在 `curve-style: bezier` のまま。蛇行後に戻りエッジが
  見づらければ `taxi` や `segments` を検討する。まず蛇行の効果を実機で見てから判断する。
- **ELK の ARD による折り返し位置最適化**: 本計画は「1行あたり固定の層数」で切る。
  分岐が密な箇所で切れて見づらい場合に、切断位置の最適化を検討する。
- **座標の複数マシン間共有**: DB はローカルファイルなので共有されない。要望が出てから扱う。
