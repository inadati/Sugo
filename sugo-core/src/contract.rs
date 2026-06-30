//! Shared contract tests for the [`HarnessRepository`] port.
//!
//! This module verifies that both implementations of the port -- the fake
//! (`InMemoryHarnessRepository`) and the sqlite-backed
//! `SqliteHarnessRepository` -- satisfy the same contract using a single set of
//! assertion bodies.
//!
//! Each `contract_*` function takes a `HarnessRepository` and asserts the
//! semantics of get/create/list/get_version/append_version (NotFound /
//! LockConflict / immutability / duplicate rejection). The core side runs them
//! against `InMemoryHarnessRepository`; sugo-infra runs the same functions
//! against `SqliteHarnessRepository`.
//!
//! The functions are `pub` and gated behind the `test-support` feature so they
//! can be called from external crates.

use crate::domain::board::BoardDefinition;
use crate::domain::cell::{Cell, CellStatus};
use crate::domain::edge::Edge;
use crate::domain::harness::{BoardVersion, Harness};
use crate::error::CoreError;
use crate::ports::id_clock::IdClock;
use crate::ports::repository::HarnessRepository;

fn sample_board(prompt: &str) -> BoardDefinition {
    BoardDefinition {
        schema_version: 1,
        start: "c1".into(),
        cells: vec![
            Cell {
                id: "c1".into(),
                name: "c1".into(),
                prompt: prompt.into(),
                status: CellStatus::Active,
                terminal: false,
            },
            Cell {
                id: "c2".into(),
                name: "c2".into(),
                prompt: "".into(),
                status: CellStatus::Active,
                terminal: true,
            },
        ],
        edges: vec![Edge {
            from: "c1".into(),
            to: "c2".into(),
            label: "ok".into(),
            guard: None,
        }],
    }
}

fn harness(id: &str, name: &str, current_version: i64, lock_version: i64) -> Harness {
    Harness {
        id: id.into(),
        name: name.into(),
        description: None,
        current_version,
        has_draft: false,
        lock_version,
        created_at: "2026-01-01T00:00:00+09:00".into(),
        updated_at: "2026-01-01T00:00:00+09:00".into(),
    }
}

fn version(id: &str, harness_id: &str, version_no: i64, def: BoardDefinition) -> BoardVersion {
    BoardVersion {
        id: id.into(),
        harness_id: harness_id.into(),
        version_no,
        content_hash: format!("hash-{version_no}"),
        definition: def,
        created_at: "2026-01-01T00:00:00+09:00".into(),
    }
}

/// create then get returns the same content.
pub async fn contract_create_get<R: HarnessRepository>(repo: &R, _clock: &dyn IdClock) {
    let h = harness("h1", "first", 1, 0);
    let v = version("v1", "h1", 1, sample_board("p1"));
    repo.create(&h, &v).await.expect("create ok");

    let (got_h, got_v) = repo.get("h1").await.expect("get ok").expect("present");
    assert_eq!(got_h.id, "h1");
    assert_eq!(got_h.name, "first");
    assert_eq!(got_h.current_version, 1);
    assert_eq!(got_v.version_no, 1);
    assert_eq!(got_v.definition.cells[0].prompt, "p1");
}

/// get of a non-existent id returns None.
pub async fn contract_get_missing_returns_none<R: HarnessRepository>(repo: &R) {
    assert!(repo.get("nope").await.expect("get ok").is_none());
}

/// append_version adds a new version without rewriting the old one, and the
/// old version's content stays immutable.
pub async fn contract_append_creates_immutable_version<R: HarnessRepository>(repo: &R) {
    let h = harness("h1", "first", 1, 0);
    let v1 = version("v1", "h1", 1, sample_board("original"));
    repo.create(&h, &v1).await.expect("create ok");

    let mut h2 = h.clone();
    h2.current_version = 2;
    h2.lock_version = 1;
    let v2 = version("v2", "h1", 2, sample_board("changed"));
    repo.append_version(&h2, &v2, 0).await.expect("append ok");

    // Old version v1 keeps its original content.
    let got_v1 = repo
        .get_version("h1", 1)
        .await
        .expect("get_version ok")
        .expect("v1 present");
    assert_eq!(got_v1.definition.cells[0].prompt, "original");

    // head moved to v2 and lock_version was persisted.
    let (got_h, got_v2) = repo.get("h1").await.expect("get ok").expect("present");
    assert_eq!(got_h.current_version, 2);
    assert_eq!(got_h.lock_version, 1);
    assert_eq!(got_v2.definition.cells[0].prompt, "changed");
}

/// appending with a mismatched expected_lock yields LockConflict.
pub async fn contract_lock_conflict<R: HarnessRepository>(repo: &R) {
    let h = harness("h1", "first", 1, 0);
    let v1 = version("v1", "h1", 1, sample_board("p1"));
    repo.create(&h, &v1).await.expect("create ok");

    let mut h2 = h.clone();
    h2.current_version = 2;
    h2.lock_version = 1;
    let v2 = version("v2", "h1", 2, sample_board("p2"));
    let err = repo
        .append_version(&h2, &v2, 99)
        .await
        .expect_err("should conflict");
    assert!(matches!(err, CoreError::LockConflict { .. }));
}

/// appending to a non-existent harness yields NotFound.
pub async fn contract_append_to_missing_harness_is_not_found<R: HarnessRepository>(repo: &R) {
    let h = harness("ghost", "x", 2, 1);
    let v = version("v2", "ghost", 2, sample_board("p"));
    let err = repo
        .append_version(&h, &v, 0)
        .await
        .expect_err("should be not found");
    assert!(matches!(err, CoreError::NotFound(_)));
}

/// create with a duplicate id returns Err instead of silently overwriting.
pub async fn contract_duplicate_id_rejected<R: HarnessRepository>(repo: &R) {
    let h = harness("h1", "first", 1, 0);
    let v = version("v1", "h1", 1, sample_board("p1"));
    repo.create(&h, &v).await.expect("create ok");

    let dup_h = harness("h1", "second", 1, 0);
    let dup_v = version("v9", "h1", 1, sample_board("other"));
    assert!(repo.create(&dup_h, &dup_v).await.is_err());

    // Original content is preserved (no silent overwrite happened).
    let (got_h, _) = repo.get("h1").await.expect("get ok").expect("present");
    assert_eq!(got_h.name, "first");
}

/// append with a duplicate version_no returns Err instead of silently
/// overwriting (immutability enforcement).
pub async fn contract_duplicate_version_no_rejected<R: HarnessRepository>(repo: &R) {
    let h = harness("h1", "first", 1, 0);
    let v1 = version("v1", "h1", 1, sample_board("original"));
    repo.create(&h, &v1).await.expect("create ok");

    // Re-append version_no=1 (with a matching lock) -> equivalent to a UNIQUE
    // violation.
    let mut h2 = h.clone();
    h2.lock_version = 1;
    let dup_v = version("v1b", "h1", 1, sample_board("overwrite"));
    assert!(repo.append_version(&h2, &dup_v, 0).await.is_err());

    // v1's content is immutable.
    let got_v1 = repo
        .get_version("h1", 1)
        .await
        .expect("get_version ok")
        .expect("v1 present");
    assert_eq!(got_v1.definition.cells[0].prompt, "original");
}

/// list returns previously created harnesses.
pub async fn contract_list_returns_created<R: HarnessRepository>(repo: &R) {
    let h = harness("h1", "first", 1, 0);
    let v = version("v1", "h1", 1, sample_board("p1"));
    repo.create(&h, &v).await.expect("create ok");

    let listed = repo.list().await.expect("list ok");
    assert!(listed.iter().any(|x| x.id == "h1"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::repository::fake::{FakeIdClock, InMemoryHarnessRepository};

    #[tokio::test]
    async fn fake_passes_create_get() {
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();
        contract_create_get(&repo, &clock).await;
    }

    #[tokio::test]
    async fn fake_passes_get_missing_returns_none() {
        let repo = InMemoryHarnessRepository::new();
        contract_get_missing_returns_none(&repo).await;
    }

    #[tokio::test]
    async fn fake_passes_append_creates_immutable_version() {
        let repo = InMemoryHarnessRepository::new();
        contract_append_creates_immutable_version(&repo).await;
    }

    #[tokio::test]
    async fn fake_passes_lock_conflict() {
        let repo = InMemoryHarnessRepository::new();
        contract_lock_conflict(&repo).await;
    }

    #[tokio::test]
    async fn fake_passes_append_to_missing_harness_is_not_found() {
        let repo = InMemoryHarnessRepository::new();
        contract_append_to_missing_harness_is_not_found(&repo).await;
    }

    #[tokio::test]
    async fn fake_passes_duplicate_id_rejected() {
        let repo = InMemoryHarnessRepository::new();
        contract_duplicate_id_rejected(&repo).await;
    }

    #[tokio::test]
    async fn fake_passes_duplicate_version_no_rejected() {
        let repo = InMemoryHarnessRepository::new();
        contract_duplicate_version_no_rejected(&repo).await;
    }

    #[tokio::test]
    async fn fake_passes_list_returns_created() {
        let repo = InMemoryHarnessRepository::new();
        contract_list_returns_created(&repo).await;
    }

    #[tokio::test]
    async fn fake_stored_content_hash_matches_recomputed() {
        // InMemoryHarnessRepository must store and return the content_hash
        // exactly as supplied, so that a recomputed hash from the retrieved
        // definition matches the stored value — mirroring the sqlite integration
        // test coverage that the fake-based tests otherwise miss.
        use crate::usecase::create_harness::content_hash;
        let repo = InMemoryHarnessRepository::new();
        let def = sample_board("determinism-test");
        let real_hash = content_hash(&def);
        let h = harness("h1", "test", 1, 0);
        let v = BoardVersion {
            id: "v1".into(),
            harness_id: "h1".into(),
            version_no: 1,
            content_hash: real_hash.clone(),
            definition: def,
            created_at: "2026-01-01T00:00:00+09:00".into(),
        };
        repo.create(&h, &v).await.expect("create ok");
        let (_, stored_v) = repo.get("h1").await.expect("get ok").expect("present");
        // The stored hash must equal the hash we supplied on create.
        assert_eq!(stored_v.content_hash, real_hash);
        // Re-computing from the retrieved definition must also match.
        assert_eq!(content_hash(&stored_v.definition), stored_v.content_hash);
    }
}
