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
use crate::domain::run::{Run, RunStatus};
use crate::error::CoreError;
use crate::ports::id_clock::IdClock;
use crate::ports::repository::HarnessRepository;
use crate::ports::run_repository::RunRepository;

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
                request_memo: "".into(),
            },
            Cell {
                id: "c2".into(),
                name: "c2".into(),
                prompt: "".into(),
                status: CellStatus::Active,
                terminal: true,
                request_memo: "".into(),
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
        folder_id: None,
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

// ── RunRepository ───────────────────────────────────────────────────────────
//
// The same fake-vs-sqlite discipline applied to the run port. Without these,
// `InMemoryRunRepository` and `SqliteRunRepository` were free to disagree: the
// fake replaced a whole stored `Run`, while sqlite wrote a narrow subset of
// columns. A usecase that assigned a field and called the broad writer then
// worked against the fake and silently did nothing in production.

/// A run with every field populated, so a writer that clobbers an unrelated
/// field is detectable rather than indistinguishable from `None`.
fn sample_run(id: &str, harness_id: &str, cell_id: &str) -> Run {
    Run {
        id: id.into(),
        harness_id: harness_id.into(),
        board_version_no: 3,
        current_cell_id: cell_id.into(),
        status: RunStatus::Running,
        project_path: Some("/abs/project".into()),
        created_at: "2026-01-01T00:00:00+09:00".into(),
        last_heartbeat_at: Some("2026-01-01T00:00:05+09:00".into()),
        updated_at: "2026-01-01T00:00:05+09:00".into(),
        inject_pending_since: Some("2026-01-01T00:00:04+09:00".into()),
        current_step_token: Some("tok-seed".into()),
    }
}

/// Every field survives a create/get round trip unchanged.
pub async fn contract_run_create_get_roundtrip<R: RunRepository>(repo: &R) {
    let run = sample_run("r1", "h1", "c1");
    repo.create(&run).await.expect("create ok");

    let got = repo.get("r1").await.expect("get ok").expect("present");
    assert_eq!(got, run, "create/get must not alter any field");
}

/// get of a non-existent run id returns None rather than erroring.
pub async fn contract_run_get_missing_returns_none<R: RunRepository>(repo: &R) {
    assert!(repo.get("nope").await.expect("get ok").is_none());
}

/// Creating the same run id twice is rejected.
pub async fn contract_run_duplicate_id_rejected<R: RunRepository>(repo: &R) {
    let run = sample_run("r1", "h1", "c1");
    repo.create(&run).await.expect("create ok");
    assert!(
        repo.create(&run).await.is_err(),
        "duplicate run id must be rejected"
    );
}

/// `set_position` writes current_cell_id, status and updated_at — and nothing else.
///
/// The "nothing else" half is the load-bearing assertion. `last_heartbeat_at`
/// is written by the callback server in a different process, so a writer that
/// persisted the whole entity would clobber it with a stale read. Every writer
/// on this port therefore names exactly the fields it touches.
pub async fn contract_run_set_position_writes_only_position_and_status<R: RunRepository>(repo: &R) {
    repo.create(&sample_run("r1", "h1", "c1"))
        .await
        .expect("create ok");

    repo.set_position("r1", "c2", RunStatus::Done, "2026-01-01T00:01:00+09:00")
        .await
        .expect("set_position ok");

    let got = repo.get("r1").await.expect("get ok").expect("present");
    // Written.
    assert_eq!(got.current_cell_id, "c2");
    assert_eq!(got.status, RunStatus::Done);
    assert_eq!(got.updated_at, "2026-01-01T00:01:00+09:00");
    // Untouched.
    assert_eq!(got.harness_id, "h1");
    assert_eq!(got.board_version_no, 3, "the pinned board must not move");
    assert_eq!(got.project_path.as_deref(), Some("/abs/project"));
    assert_eq!(got.created_at, "2026-01-01T00:00:00+09:00");
    assert_eq!(
        got.last_heartbeat_at.as_deref(),
        Some("2026-01-01T00:00:05+09:00"),
        "must not clear a heartbeat written by another process"
    );
    assert_eq!(
        got.inject_pending_since.as_deref(),
        Some("2026-01-01T00:00:04+09:00"),
        "must not clear the inject gate; use set_inject_pending"
    );
    assert_eq!(
        got.current_step_token.as_deref(),
        Some("tok-seed"),
        "must not consume the step token; use set_step_token"
    );
}

/// `set_status` writes status and updated_at — and nothing else, in particular
/// not the run's position.
pub async fn contract_run_set_status_writes_only_status<R: RunRepository>(repo: &R) {
    repo.create(&sample_run("r1", "h1", "c1"))
        .await
        .expect("create ok");

    repo.set_status("r1", RunStatus::Closed, "2026-01-01T00:01:00+09:00")
        .await
        .expect("set_status ok");

    let got = repo.get("r1").await.expect("get ok").expect("present");
    assert_eq!(got.status, RunStatus::Closed);
    assert_eq!(got.updated_at, "2026-01-01T00:01:00+09:00");
    assert_eq!(
        got.current_cell_id, "c1",
        "the cell the run died on is kept as history"
    );
    assert_eq!(
        got.last_heartbeat_at.as_deref(),
        Some("2026-01-01T00:00:05+09:00")
    );
    assert_eq!(
        got.current_step_token.as_deref(),
        Some("tok-seed"),
        "set_status alone must not consume the step token"
    );
}

/// The position/status writers report NotFound for an unknown run id.
///
/// Unlike the fire-and-forget setters, these back usecases that must fail
/// loudly rather than pretend a missing run was advanced or stopped.
pub async fn contract_run_writers_report_missing_run<R: RunRepository>(repo: &R) {
    let err = repo
        .set_position(
            "nope",
            "c2",
            RunStatus::Running,
            "2026-01-01T00:01:00+09:00",
        )
        .await
        .expect_err("set_position on a missing run must fail");
    assert!(
        matches!(err, CoreError::NotFound(_)),
        "expected NotFound, got {err:?}"
    );

    let err = repo
        .set_status("nope", RunStatus::Closed, "2026-01-01T00:01:00+09:00")
        .await
        .expect_err("set_status on a missing run must fail");
    assert!(
        matches!(err, CoreError::NotFound(_)),
        "expected NotFound, got {err:?}"
    );
}

/// `update_heartbeat` writes only last_heartbeat_at.
pub async fn contract_run_heartbeat_writes_only_heartbeat<R: RunRepository>(repo: &R) {
    repo.create(&sample_run("r1", "h1", "c1"))
        .await
        .expect("create ok");
    repo.update_heartbeat("r1", "2026-01-01T00:02:00+09:00")
        .await
        .expect("heartbeat ok");

    let got = repo.get("r1").await.expect("get ok").expect("present");
    assert_eq!(
        got.last_heartbeat_at.as_deref(),
        Some("2026-01-01T00:02:00+09:00")
    );
    assert_eq!(got.current_cell_id, "c1");
    assert_eq!(got.status, RunStatus::Running);
    assert_eq!(got.updated_at, "2026-01-01T00:00:05+09:00");
}

/// `set_inject_pending` round-trips Some then None, touching nothing else.
pub async fn contract_run_set_inject_pending_roundtrip<R: RunRepository>(repo: &R) {
    repo.create(&sample_run("r1", "h1", "c1"))
        .await
        .expect("create ok");

    repo.set_inject_pending("r1", Some("2026-01-01T00:03:00+09:00"))
        .await
        .expect("set ok");
    let got = repo.get("r1").await.expect("get ok").expect("present");
    assert_eq!(
        got.inject_pending_since.as_deref(),
        Some("2026-01-01T00:03:00+09:00")
    );
    assert_eq!(got.current_step_token.as_deref(), Some("tok-seed"));

    repo.set_inject_pending("r1", None).await.expect("clear ok");
    let got = repo.get("r1").await.expect("get ok").expect("present");
    assert_eq!(got.inject_pending_since, None);
    assert_eq!(
        got.current_step_token.as_deref(),
        Some("tok-seed"),
        "clearing the inject gate must not consume the step token"
    );
}

/// `set_step_token` round-trips Some then None, touching nothing else.
pub async fn contract_run_set_step_token_roundtrip<R: RunRepository>(repo: &R) {
    repo.create(&sample_run("r1", "h1", "c1"))
        .await
        .expect("create ok");

    repo.set_step_token("r1", Some("tok-next"))
        .await
        .expect("set ok");
    let got = repo.get("r1").await.expect("get ok").expect("present");
    assert_eq!(got.current_step_token.as_deref(), Some("tok-next"));

    repo.set_step_token("r1", None).await.expect("clear ok");
    let got = repo.get("r1").await.expect("get ok").expect("present");
    assert_eq!(got.current_step_token, None);
    assert_eq!(
        got.inject_pending_since.as_deref(),
        Some("2026-01-01T00:00:04+09:00"),
        "consuming the step token must not clear the inject gate"
    );
}

/// The narrow setters are no-ops on an unknown run id, not errors.
///
/// They are called from fire-and-forget paths (the callback HTTP handlers),
/// where a run that has since been purged must not turn into an error path.
pub async fn contract_run_setters_ignore_missing_run<R: RunRepository>(repo: &R) {
    repo.update_heartbeat("nope", "2026-01-01T00:02:00+09:00")
        .await
        .expect("heartbeat on missing run is a no-op");
    repo.set_inject_pending("nope", Some("2026-01-01T00:02:00+09:00"))
        .await
        .expect("set_inject_pending on missing run is a no-op");
    repo.set_step_token("nope", Some("tok"))
        .await
        .expect("set_step_token on missing run is a no-op");
}

/// `list_by_harness` returns only that harness's runs, newest first.
pub async fn contract_run_list_by_harness_is_newest_first<R: RunRepository>(repo: &R) {
    let mut older = sample_run("r-old", "h1", "c1");
    older.created_at = "2026-01-01T00:00:00+09:00".into();
    let mut newer = sample_run("r-new", "h1", "c2");
    newer.created_at = "2026-01-02T00:00:00+09:00".into();
    let other = sample_run("r-other", "h2", "c1");
    repo.create(&older).await.expect("create ok");
    repo.create(&newer).await.expect("create ok");
    repo.create(&other).await.expect("create ok");

    let listed = repo.list_by_harness("h1").await.expect("list ok");
    let ids: Vec<&str> = listed.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, vec!["r-new", "r-old"]);
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

/// The run contract, run against `InMemoryRunRepository`.
///
/// sugo-infra runs the same functions against `SqliteRunRepository`; any
/// behaviour the two do not share shows up here or there rather than as a
/// usecase that works in tests and does nothing in production.
#[cfg(test)]
mod run_tests {
    use super::*;
    use crate::ports::run_repository::fake::InMemoryRunRepository;

    fn repo() -> InMemoryRunRepository {
        InMemoryRunRepository::new()
    }

    #[tokio::test]
    async fn fake_passes_create_get_roundtrip() {
        contract_run_create_get_roundtrip(&repo()).await;
    }

    #[tokio::test]
    async fn fake_passes_get_missing_returns_none() {
        contract_run_get_missing_returns_none(&repo()).await;
    }

    #[tokio::test]
    async fn fake_passes_duplicate_id_rejected() {
        contract_run_duplicate_id_rejected(&repo()).await;
    }

    #[tokio::test]
    async fn fake_passes_set_position_writes_only_position_and_status() {
        contract_run_set_position_writes_only_position_and_status(&repo()).await;
    }

    #[tokio::test]
    async fn fake_passes_set_status_writes_only_status() {
        contract_run_set_status_writes_only_status(&repo()).await;
    }

    #[tokio::test]
    async fn fake_passes_writers_report_missing_run() {
        contract_run_writers_report_missing_run(&repo()).await;
    }

    #[tokio::test]
    async fn fake_passes_heartbeat_writes_only_heartbeat() {
        contract_run_heartbeat_writes_only_heartbeat(&repo()).await;
    }

    #[tokio::test]
    async fn fake_passes_set_inject_pending_roundtrip() {
        contract_run_set_inject_pending_roundtrip(&repo()).await;
    }

    #[tokio::test]
    async fn fake_passes_set_step_token_roundtrip() {
        contract_run_set_step_token_roundtrip(&repo()).await;
    }

    #[tokio::test]
    async fn fake_passes_setters_ignore_missing_run() {
        contract_run_setters_ignore_missing_run(&repo()).await;
    }

    #[tokio::test]
    async fn fake_passes_list_by_harness_is_newest_first() {
        contract_run_list_by_harness_is_newest_first(&repo()).await;
    }
}
