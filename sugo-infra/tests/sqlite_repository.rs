//! Integration tests for [`SqliteHarnessRepository`] driven entirely through
//! the public [`HarnessRepository`] port.
//!
//! Covers create/get round-trips, board-version immutability, monotonic
//! `version_no`, `UNIQUE`/lock-conflict rejection, transaction rollback, the
//! application-level pre-SELECT `NotFound` guard, and the shared cross-crate
//! contract suite run against the sqlite adapter. Connection-internal wiring
//! that needs the private `Connection` (the FK regression guard, the WAL/
//! `journal_mode` branch, `busy_timeout` contention, poison recovery) lives in
//! the in-crate unit tests in `src/sqlite/repository.rs`.

use sugo_core::ports::repository::HarnessRepository;
use sugo_infra::sqlite::SqliteHarnessRepository;

mod helpers {
    use sugo_core::domain::board::BoardDefinition;
    use sugo_core::domain::cell::{Cell, CellStatus};
    use sugo_core::domain::edge::{Edge, Guard};
    use sugo_core::domain::harness::{BoardVersion, Harness};
    use sugo_core::usecase::create_harness::content_hash;

    /// Builds a minimal board carrying the given prompt.
    pub fn board(prompt: &str) -> BoardDefinition {
        BoardDefinition {
            schema_version: 1,
            start: "c1".into(),
            cells: vec![Cell {
                id: "c1".into(),
                name: "c1".into(),
                prompt: prompt.into(),
                status: CellStatus::Active,
                terminal: true,
                request_memo: "".into(),
            }],
            edges: vec![],
        }
    }

    /// A richer board including an edge with guard=Some(...) and a draft cell.
    pub fn rich_board() -> BoardDefinition {
        BoardDefinition {
            schema_version: 1,
            start: "c1".into(),
            cells: vec![
                Cell {
                    id: "c1".into(),
                    name: "intro".into(),
                    prompt: "hello".into(),
                    status: CellStatus::Active,
                    terminal: false,
                    request_memo: "".into(),
                },
                Cell {
                    id: "c2".into(),
                    name: "draft-cell".into(),
                    prompt: "wip".into(),
                    status: CellStatus::Draft,
                    terminal: false,
                    request_memo: "".into(),
                },
                Cell {
                    id: "c3".into(),
                    name: "done".into(),
                    prompt: "".into(),
                    status: CellStatus::Active,
                    terminal: true,
                    request_memo: "".into(),
                },
            ],
            edges: vec![
                Edge {
                    from: "c1".into(),
                    to: "c2".into(),
                    label: "next".into(),
                    guard: Some(Guard { expr: "score > 0".into() }),
                },
                Edge {
                    from: "c2".into(),
                    to: "c3".into(),
                    label: "finish".into(),
                    guard: None,
                },
            ],
        }
    }

    /// Builds a BoardVersion with a real content_hash computed from the board.
    pub fn version(id: &str, harness_id: &str, version_no: i64, def: BoardDefinition) -> BoardVersion {
        let content_hash = content_hash(&def);
        BoardVersion {
            id: id.into(),
            harness_id: harness_id.into(),
            version_no,
            content_hash,
            definition: def,
            created_at: "t".into(),
        }
    }

    pub fn harness(id: &str, current_version: i64, lock_version: i64) -> Harness {
        Harness {
            id: id.into(),
            name: "h".into(),
            description: None,
            folder_id: None,
            current_version,
            has_draft: false,
            lock_version,
            created_at: "t".into(),
            updated_at: "t".into(),
        }
    }

    /// Returns h1 / v1 (prompt="orig") ready to be inserted into a repo.
    pub fn sample() -> (Harness, BoardVersion) {
        (harness("h1", 1, 0), version("v1", "h1", 1, board("orig")))
    }
}

use helpers::{board, harness, rich_board, version};
use sugo_core::usecase::create_harness::content_hash;

#[tokio::test]
async fn create_and_get_roundtrip() {
    let repo = SqliteHarnessRepository::in_memory().unwrap();
    let (h, v) = helpers::sample();
    repo.create(&h, &v).await.unwrap();
    let (gh, gv) = repo.get("h1").await.unwrap().unwrap();
    assert_eq!(gh.name, "h");
    assert_eq!(gv.version_no, 1);
    assert_eq!(gv.definition.cells[0].id, "c1");
}

#[tokio::test]
async fn append_version_enforces_lock() {
    let repo = SqliteHarnessRepository::in_memory().unwrap();
    let (mut h, v) = helpers::sample();
    repo.create(&h, &v).await.unwrap();

    let v2 = version("v2", "h1", 2, board("v2-prompt"));
    h.current_version = 2;
    h.lock_version = 1;

    // A wrong expected_lock conflicts.
    let err = repo.append_version(&h, &v2, 99).await.unwrap_err();
    assert!(matches!(
        err,
        sugo_core::error::CoreError::LockConflict { .. }
    ));

    // The correct expected_lock succeeds.
    repo.append_version(&h, &v2, 0).await.unwrap();
    let stored = repo.get_version("h1", 2).await.unwrap().unwrap();
    assert_eq!(stored.version_no, 2);
}

/// (a) Immutability: appending v2 with different content leaves v1's prompt
/// unchanged.
#[tokio::test]
async fn old_version_content_is_immutable_after_append() {
    let repo = SqliteHarnessRepository::in_memory().unwrap();
    let (mut h, v1) = helpers::sample(); // v1 prompt = "orig"
    repo.create(&h, &v1).await.unwrap();

    // Append a v2 whose prompt actually differs.
    let v2 = version("v2", "h1", 2, board("CHANGED"));
    h.current_version = 2;
    h.lock_version = 1;
    repo.append_version(&h, &v2, 0).await.unwrap();

    // After the append v1's definition cell prompt is still the original ("orig").
    let stored_v1 = repo.get_version("h1", 1).await.unwrap().unwrap();
    assert_eq!(stored_v1.definition.cells[0].prompt, "orig");
    // v2 carries the new value.
    let stored_v2 = repo.get_version("h1", 2).await.unwrap().unwrap();
    assert_eq!(stored_v2.definition.cells[0].prompt, "CHANGED");
}

/// (b) version_no increases monotonically per harness: 1, 2, 3.
#[tokio::test]
async fn version_no_increases_monotonically() {
    let repo = SqliteHarnessRepository::in_memory().unwrap();
    let (mut h, v1) = helpers::sample();
    repo.create(&h, &v1).await.unwrap();
    assert_eq!(repo.get_version("h1", 1).await.unwrap().unwrap().version_no, 1);

    let v2 = version("v2", "h1", 2, board("p2"));
    h.current_version = 2;
    h.lock_version = 1;
    repo.append_version(&h, &v2, 0).await.unwrap();
    assert_eq!(repo.get_version("h1", 2).await.unwrap().unwrap().version_no, 2);

    let v3 = version("v3", "h1", 3, board("p3"));
    h.current_version = 3;
    h.lock_version = 2;
    repo.append_version(&h, &v3, 1).await.unwrap();
    assert_eq!(repo.get_version("h1", 3).await.unwrap().unwrap().version_no, 3);
}

/// (c) UNIQUE(harness_id, version_no): appending a duplicate version_no yields a
/// Storage error.
#[tokio::test]
async fn duplicate_version_no_is_rejected() {
    let repo = SqliteHarnessRepository::in_memory().unwrap();
    let (mut h, v1) = helpers::sample();
    repo.create(&h, &v1).await.unwrap();

    // Re-append version_no=1 (different id, matching lock) -> UNIQUE violation.
    let dup = version("v1b", "h1", 1, board("overwrite"));
    h.lock_version = 1;
    let err = repo.append_version(&h, &dup, 0).await.unwrap_err();
    assert!(matches!(err, sugo_core::error::CoreError::Storage(_)));

    // v1 is unchanged.
    let stored_v1 = repo.get_version("h1", 1).await.unwrap().unwrap();
    assert_eq!(stored_v1.definition.cells[0].prompt, "orig");
}

/// (d) lock_version persistence: after a successful append, get reads back
/// lock_version=1.
#[tokio::test]
async fn lock_version_is_persisted_after_append() {
    let repo = SqliteHarnessRepository::in_memory().unwrap();
    let (mut h, v1) = helpers::sample();
    repo.create(&h, &v1).await.unwrap();

    let v2 = version("v2", "h1", 2, board("p2"));
    h.current_version = 2;
    h.lock_version = 1;
    repo.append_version(&h, &v2, 0).await.unwrap();

    let (gh, _) = repo.get("h1").await.unwrap().unwrap();
    assert_eq!(gh.lock_version, 1);
    assert_eq!(gh.current_version, 2);
}

/// (e) stale-lock regression: reusing an already-consumed expected_lock_version
/// on the second append yields LockConflict.
#[tokio::test]
async fn reusing_stale_lock_version_conflicts() {
    let repo = SqliteHarnessRepository::in_memory().unwrap();
    let (mut h, v1) = helpers::sample();
    repo.create(&h, &v1).await.unwrap();

    // First call: expected_lock=0 succeeds (lock_version becomes 1).
    let v2 = version("v2", "h1", 2, board("p2"));
    h.current_version = 2;
    h.lock_version = 1;
    repo.append_version(&h, &v2, 0).await.unwrap();

    // Second call: reuse the stale expected_lock=0 (with a fresh version_no) -> conflict.
    let v3 = version("v3", "h1", 3, board("p3"));
    h.current_version = 3;
    h.lock_version = 2;
    let err = repo.append_version(&h, &v3, 0).await.unwrap_err();
    assert!(matches!(
        err,
        sugo_core::error::CoreError::LockConflict { .. }
    ));

    // Because it conflicted, v3 was not persisted and head is still v2.
    assert!(repo.get_version("h1", 3).await.unwrap().is_none());
    let (gh, _) = repo.get("h1").await.unwrap().unwrap();
    assert_eq!(gh.current_version, 2);
    assert_eq!(gh.lock_version, 1);
}

/// (f) get of a nonexistent id returns None.
#[tokio::test]
async fn get_missing_returns_none() {
    let repo = SqliteHarnessRepository::in_memory().unwrap();
    assert!(repo.get("nope").await.unwrap().is_none());
}

/// (g) content_hash round-trip (minimal case): the hash recomputed after fetch
/// matches the stored value.
#[tokio::test]
async fn content_hash_roundtrips_minimal() {
    let repo = SqliteHarnessRepository::in_memory().unwrap();
    let (h, v) = helpers::sample();
    let expected = v.content_hash.clone();
    repo.create(&h, &v).await.unwrap();

    let stored = repo.get_version("h1", 1).await.unwrap().unwrap();
    // The stored value is the real hash, not a fake placeholder.
    assert_eq!(stored.content_hash, expected);
    // The hash recomputed from the fetched definition matches the stored value.
    assert_eq!(content_hash(&stored.definition), stored.content_hash);
}

/// (g) content_hash round-trip (case with guard=Some / draft cell on edges).
#[tokio::test]
async fn content_hash_roundtrips_rich_definition() {
    let repo = SqliteHarnessRepository::in_memory().unwrap();
    let def = rich_board();
    let h = harness("h2", 1, 0);
    let v = version("rv1", "h2", 1, def);
    let expected = v.content_hash.clone();
    repo.create(&h, &v).await.unwrap();

    let stored = repo.get_version("h2", 1).await.unwrap().unwrap();
    assert_eq!(stored.content_hash, expected);
    // The recomputed hash still matches for a definition containing guard / draft.
    assert_eq!(content_hash(&stored.definition), stored.content_hash);
    // guard and draft survive the definition round-trip.
    assert!(stored.definition.edges[0].guard.is_some());
    assert_eq!(
        stored.definition.cells[1].status,
        sugo_core::domain::cell::CellStatus::Draft
    );
}

/// Application-level pre-SELECT guard: `append_version` for a harness that does
/// not exist returns `NotFound` (from its pre-SELECT) and persists nothing. This
/// asserts the public-API guard only; the FK constraint itself is regression-
/// guarded by the raw orphan-INSERT unit test in `src/sqlite/repository.rs`,
/// which bypasses this pre-SELECT to exercise `PRAGMA foreign_keys = ON`.
#[tokio::test]
async fn append_to_missing_harness_returns_not_found() {
    let repo = SqliteHarnessRepository::in_memory().unwrap();
    // No harness "ghost" exists, so appending a board_version for it must fail.
    let mut h = harness("ghost", 2, 1);
    h.current_version = 2;
    let v = version("orphan", "ghost", 2, board("x"));
    let err = repo.append_version(&h, &v, 0).await.unwrap_err();
    // append_version's pre-SELECT reports the missing parent as NotFound.
    assert!(matches!(err, sugo_core::error::CoreError::NotFound(_)));
    // The orphan version was not persisted.
    assert!(repo.get_version("ghost", 2).await.unwrap().is_none());
}

/// Concurrency: two writers race the same expected_lock_version. Exactly one
/// append succeeds and the other returns LockConflict; head and lock_version
/// stay consistent with no partial state left behind.
#[tokio::test]
async fn concurrent_appends_one_wins_other_conflicts() {
    use std::sync::Arc;

    let repo = Arc::new(SqliteHarnessRepository::in_memory().unwrap());
    let (h, v1) = helpers::sample();
    repo.create(&h, &v1).await.unwrap();

    // Two writers both target expected_lock=0 but supply distinct version_no.
    let r1 = Arc::clone(&repo);
    let r2 = Arc::clone(&repo);

    let mut ha = harness("h1", 2, 1);
    ha.current_version = 2;
    let va = version("vA", "h1", 2, board("A"));

    let mut hb = harness("h1", 3, 1);
    hb.current_version = 3;
    let vb = version("vB", "h1", 3, board("B"));

    let t1 = tokio::spawn(async move { r1.append_version(&ha, &va, 0).await });
    let t2 = tokio::spawn(async move { r2.append_version(&hb, &vb, 0).await });

    let res1 = t1.await.unwrap();
    let res2 = t2.await.unwrap();

    // Exactly one succeeded.
    let ok_count = [&res1, &res2].iter().filter(|r| r.is_ok()).count();
    let conflict_count = [&res1, &res2]
        .iter()
        .filter(|r| matches!(r, Err(sugo_core::error::CoreError::LockConflict { .. })))
        .count();
    assert_eq!(ok_count, 1, "exactly one writer must win");
    assert_eq!(conflict_count, 1, "the other must see LockConflict");

    // Head is consistent: lock_version advanced to 1 and current_version points
    // at whichever winner's version, which is the only extra version persisted.
    let (gh, _) = repo.get("h1").await.unwrap().unwrap();
    assert_eq!(gh.lock_version, 1);
    assert!(gh.current_version == 2 || gh.current_version == 3);
    let stored_winner = repo
        .get_version("h1", gh.current_version)
        .await
        .unwrap();
    assert!(stored_winner.is_some());
    // The loser's version was not persisted (no partial state).
    let loser_no = if gh.current_version == 2 { 3 } else { 2 };
    assert!(repo.get_version("h1", loser_no).await.unwrap().is_none());
}

/// Transaction rollback on UNIQUE violation: when the lock check passes but the
/// board_version INSERT hits UNIQUE(harness_id, version_no), the whole
/// transaction rolls back — head is not advanced and no partial commit remains.
#[tokio::test]
async fn unique_violation_after_lock_check_rolls_back() {
    let repo = SqliteHarnessRepository::in_memory().unwrap();
    let (h, v1) = helpers::sample(); // v1 at version_no=1, lock_version=0
    repo.create(&h, &v1).await.unwrap();

    // Append a real v2 to move head to version_no=2 and lock_version to 1.
    let mut h2 = harness("h1", 2, 1);
    h2.current_version = 2;
    let v2 = version("v2", "h1", 2, board("p2"));
    repo.append_version(&h2, &v2, 0).await.unwrap();

    // Now attempt an append that passes the lock check (expected_lock=1) but
    // collides on version_no=2 (UNIQUE violation) while trying to advance head.
    let mut h3 = harness("h1", 3, 2);
    h3.current_version = 3;
    let dup = version("v2dup", "h1", 2, board("collide"));
    let err = repo.append_version(&h3, &dup, 1).await.unwrap_err();
    assert!(matches!(err, sugo_core::error::CoreError::Storage(_)));

    // The transaction rolled back: head is unchanged (still v2, lock_version=1).
    let (gh, _) = repo.get("h1").await.unwrap().unwrap();
    assert_eq!(gh.current_version, 2);
    assert_eq!(gh.lock_version, 1);
    // No phantom version_no=3 was committed.
    assert!(repo.get_version("h1", 3).await.unwrap().is_none());
    // v2 remains its original content.
    let stored_v2 = repo.get_version("h1", 2).await.unwrap().unwrap();
    assert_eq!(stored_v2.definition.cells[0].prompt, "p2");
}

// --- contract-test-parity: run sugo-core's shared contract functions against
// SqliteHarnessRepository ---
// Mechanically guarantees that the fake (InMemoryHarnessRepository) and sqlite
// pass the same ports contract.

mod contract {
    use super::SqliteHarnessRepository;
    use sugo_core::contract;
    use sugo_core::ports::repository::fake::FakeIdClock;

    fn repo() -> SqliteHarnessRepository {
        SqliteHarnessRepository::in_memory().unwrap()
    }

    #[tokio::test]
    async fn sqlite_passes_create_get() {
        contract::contract_create_get(&repo(), &FakeIdClock::new()).await;
    }

    #[tokio::test]
    async fn sqlite_passes_get_missing_returns_none() {
        contract::contract_get_missing_returns_none(&repo()).await;
    }

    #[tokio::test]
    async fn sqlite_passes_append_creates_immutable_version() {
        contract::contract_append_creates_immutable_version(&repo()).await;
    }

    #[tokio::test]
    async fn sqlite_passes_lock_conflict() {
        contract::contract_lock_conflict(&repo()).await;
    }

    #[tokio::test]
    async fn sqlite_passes_append_to_missing_harness_is_not_found() {
        contract::contract_append_to_missing_harness_is_not_found(&repo()).await;
    }

    #[tokio::test]
    async fn sqlite_passes_duplicate_id_rejected() {
        contract::contract_duplicate_id_rejected(&repo()).await;
    }

    #[tokio::test]
    async fn sqlite_passes_duplicate_version_no_rejected() {
        contract::contract_duplicate_version_no_rejected(&repo()).await;
    }

    #[tokio::test]
    async fn sqlite_passes_list_returns_created() {
        contract::contract_list_returns_created(&repo()).await;
    }
}
