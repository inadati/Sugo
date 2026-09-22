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
///
/// `cell_positions.harness_id` は `harnesses(id)` への外部キーなので、テストが
/// 使う "h1"/"h2" の親行をあらかじめ最小構成で作っておく。この環境の bundled
/// SQLite は `PRAGMA foreign_keys` が接続ごとに明示設定せずとも既定で ON になる
/// ため、親行が無いと INSERT が FK 違反で弾かれる。
fn repo() -> (tempfile::TempDir, SqliteCellPositionRepository) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("sugo.db");
    let path_str = path.to_string_lossy().into_owned();
    SqliteHarnessRepository::open(&path_str).expect("schema applied");
    let conn = rusqlite::Connection::open(&path_str).expect("open");
    for id in ["h1", "h2"] {
        conn.execute(
            "INSERT INTO harnesses \
             (id, name, description, folder_id, current_version, has_draft, lock_version, created_at, updated_at) \
             VALUES (?1, ?1, NULL, NULL, 1, 0, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [id],
        )
        .expect("seed harness");
    }
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
