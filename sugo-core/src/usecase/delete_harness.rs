//! Use case for moving a harness to the trash (soft delete).
//!
//! Refuses to delete while an active run exists for the harness, mirroring
//! sugo-gui's `trash_harness` Tauri command's active-run check so both entry
//! points share the same rule instead of silently diverging.

use crate::domain::run::RunStatus;
use crate::error::CoreError;
use crate::ports::id_clock::IdClock;
use crate::ports::repository::HarnessRepository;
use crate::ports::run_repository::RunRepository;

/// A Running run counts as active if its heartbeat/updated_at is within this
/// many seconds of "now" (matches sugo-gui's `trash_harness`/`get_active_runs`).
const ACTIVE_RUN_STALE_SECS: i64 = 300;

/// Input for [`delete_harness`].
pub struct DeleteHarnessInput {
    /// Id of the harness to move to the trash.
    pub harness_id: String,
}

/// Moves a harness to the trash (sets `deleted_at`), refusing if an active
/// run exists.
pub async fn delete_harness(
    harness_repo: &dyn HarnessRepository,
    run_repo: &dyn RunRepository,
    clock: &dyn IdClock,
    input: DeleteHarnessInput,
) -> Result<(), CoreError> {
    let runs = run_repo.list_by_harness(&input.harness_id).await?;
    let now_iso = clock.now_iso();
    let now = chrono::DateTime::parse_from_rfc3339(&now_iso)
        .map_err(|e| CoreError::Storage(format!("invalid now_iso: {e}")))?
        .with_timezone(&chrono::Utc);

    let has_active_run = runs
        .iter()
        .filter(|r| r.status == RunStatus::Running)
        .any(|r| {
            let ts = r.last_heartbeat_at.as_deref().unwrap_or(&r.updated_at);
            chrono::DateTime::parse_from_rfc3339(ts)
                .map(|dt| (now - dt.with_timezone(&chrono::Utc)).num_seconds() < ACTIVE_RUN_STALE_SECS)
                .unwrap_or(false)
        });
    if has_active_run {
        return Err(CoreError::ActiveRunExists);
    }

    harness_repo.trash_harness(&input.harness_id, &now_iso).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::run::Run;
    use crate::ports::repository::fake::{FakeIdClock, InMemoryHarnessRepository};
    use crate::ports::run_repository::fake::InMemoryRunRepository;
    use crate::usecase::create_harness::{CreateHarnessInput, create_harness};

    async fn seed() -> (InMemoryHarnessRepository, InMemoryRunRepository, FakeIdClock, String) {
        let repo = InMemoryHarnessRepository::new();
        let run_repo = InMemoryRunRepository::new();
        let clock = FakeIdClock::new();
        let out = create_harness(
            &repo,
            &clock,
            CreateHarnessInput { name: "h".into(), description: None, definition: None },
        )
        .await
        .unwrap();
        (repo, run_repo, clock, out.harness_id)
    }

    /// A Running run for `harness_id`, last touched at `updated_at` (RFC3339).
    /// `FakeIdClock::now_iso()` always returns "2026-01-01T00:00:00+09:00", so
    /// tests express "active" vs. "stale" relative to that fixed instant.
    fn running_run(harness_id: &str, updated_at: &str) -> Run {
        Run {
            id: "r1".into(),
            harness_id: harness_id.into(),
            board_version_no: 1,
            current_cell_id: "start".into(),
            status: RunStatus::Running,
            project_path: Some("/abs/p".into()),
            created_at: "2026-01-01T00:00:00+09:00".into(),
            last_heartbeat_at: None,
            updated_at: updated_at.into(),
            inject_pending_since: None,
        }
    }

    #[tokio::test]
    async fn delete_moves_harness_to_trash_when_no_run_exists() {
        let (repo, run_repo, clock, id) = seed().await;
        delete_harness(&repo, &run_repo, &clock, DeleteHarnessInput { harness_id: id.clone() })
            .await
            .unwrap();
        assert!(repo.list_trash().await.unwrap().iter().any(|(hid, _, _)| hid == &id));
        assert!(repo.list().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn delete_blocked_by_active_run_within_window() {
        let (repo, run_repo, clock, id) = seed().await;
        run_repo.create(&running_run(&id, "2026-01-01T00:00:00+09:00")).await.unwrap();
        let err = delete_harness(&repo, &run_repo, &clock, DeleteHarnessInput { harness_id: id })
            .await
            .unwrap_err();
        assert!(matches!(err, CoreError::ActiveRunExists));
    }

    #[tokio::test]
    async fn delete_allows_when_active_run_is_stale() {
        let (repo, run_repo, clock, id) = seed().await;
        // 12 hours before the fixed clock instant: well past the 300s window.
        run_repo.create(&running_run(&id, "2025-12-31T12:00:00+09:00")).await.unwrap();
        delete_harness(&repo, &run_repo, &clock, DeleteHarnessInput { harness_id: id.clone() })
            .await
            .unwrap();
        assert!(repo.list_trash().await.unwrap().iter().any(|(hid, _, _)| hid == &id));
    }

    #[tokio::test]
    async fn delete_ignores_non_running_run() {
        let (repo, run_repo, clock, id) = seed().await;
        let mut r = running_run(&id, "2026-01-01T00:00:00+09:00");
        r.status = RunStatus::Done;
        run_repo.create(&r).await.unwrap();
        delete_harness(&repo, &run_repo, &clock, DeleteHarnessInput { harness_id: id.clone() })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn delete_missing_harness_is_not_found() {
        let repo = InMemoryHarnessRepository::new();
        let run_repo = InMemoryRunRepository::new();
        let clock = FakeIdClock::new();
        let err = delete_harness(
            &repo,
            &run_repo,
            &clock,
            DeleteHarnessInput { harness_id: "ghost".into() },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, CoreError::NotFound(_)));
    }
}
