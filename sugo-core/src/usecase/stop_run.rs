//! Use case for stopping a run that is in flight.
//!
//! Stopping ends the run without touching the harness definition, so the same
//! harness can immediately be started again with `start_run`. This is the
//! escape hatch for a run that has reached a dead end: the agent is standing
//! on a cell whose outgoing edges all describe the wrong next step (e.g. an
//! earlier cell's artifact turned out to be wrong and no edge leads back to
//! it), so `advance_run` must not be called at all.
//!
//! The run row is kept as history with the `current_cell_id` it died on;
//! callers that list live runs filter on [`RunStatus::Running`], so a stopped
//! run disappears from those views on its own.

use crate::domain::run::RunStatus;
use crate::error::CoreError;
use crate::ports::id_clock::IdClock;
use crate::ports::run_repository::RunRepository;

#[derive(Debug)]
pub struct StopRunInput {
    pub run_id: String,
}

#[derive(Debug, PartialEq)]
pub struct StopRunOutput {
    /// The cell the run was standing on when it was stopped.
    pub stopped_at_cell_id: String,
    /// The harness the stopped run belonged to, so the caller can offer a restart.
    pub harness_id: String,
    /// False when the run had already finished, so nothing was changed.
    pub was_in_flight: bool,
}

/// Stop the run `run_id`, leaving its harness untouched.
///
/// Moves a `Running` / `Stalled` / `Disconnected` run to
/// [`RunStatus::Closed`] and clears the inject gate
/// (`inject_pending_since`) and the outstanding one-time step token, so no
/// leftover token can authorize an advance on a run that is no longer live.
///
/// Idempotent: a run that already reached `Done` or `Closed` is left exactly
/// as it is and reported with `was_in_flight: false`. Fails with
/// [`CoreError::NotFound`] when the run does not exist.
pub async fn stop_run(
    run_repo: &dyn RunRepository,
    clock: &dyn IdClock,
    input: StopRunInput,
) -> Result<StopRunOutput, CoreError> {
    let run = run_repo
        .get(&input.run_id)
        .await?
        .ok_or_else(|| CoreError::NotFound(format!("run not found: {}", input.run_id)))?;

    let was_in_flight = matches!(
        run.status,
        RunStatus::Running | RunStatus::Stalled | RunStatus::Disconnected
    );

    if was_in_flight {
        run_repo
            .set_status(&run.id, RunStatus::Closed, &clock.now_iso())
            .await?;
        // Each writer names the fields it touches, so the inject gate and the
        // step token are cleared explicitly rather than as a side effect of
        // persisting the entity.
        run_repo.set_inject_pending(&run.id, None).await?;
        run_repo.set_step_token(&run.id, None).await?;
    }

    Ok(StopRunOutput {
        stopped_at_cell_id: run.current_cell_id,
        harness_id: run.harness_id,
        was_in_flight,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::run::Run;
    use crate::ports::repository::fake::FakeIdClock;
    use crate::ports::run_repository::fake::InMemoryRunRepository;

    fn run_with(status: RunStatus) -> Run {
        Run {
            id: "r1".into(),
            harness_id: "h1".into(),
            board_version_no: 1,
            current_cell_id: "c10".into(),
            status,
            project_path: Some("/p".into()),
            created_at: "2026-09-21T00:00:00+09:00".into(),
            last_heartbeat_at: None,
            updated_at: "2026-09-21T00:00:00+09:00".into(),
            inject_pending_since: Some("2026-09-21T00:00:00+09:00".into()),
            current_step_token: Some("tok-abc".into()),
        }
    }

    async fn repo_with(status: RunStatus) -> InMemoryRunRepository {
        let repo = InMemoryRunRepository::new();
        repo.create(&run_with(status)).await.unwrap();
        repo
    }

    #[tokio::test]
    async fn stopping_running_run_closes_it_and_clears_the_gates() {
        let repo = repo_with(RunStatus::Running).await;
        let clock = FakeIdClock::new();

        let out = stop_run(
            &repo,
            &clock,
            StopRunInput {
                run_id: "r1".into(),
            },
        )
        .await
        .unwrap();

        assert_eq!(out.stopped_at_cell_id, "c10");
        assert_eq!(out.harness_id, "h1");
        assert!(out.was_in_flight);

        let run = repo.get("r1").await.unwrap().unwrap();
        assert_eq!(run.status, RunStatus::Closed);
        // A leftover token must not be able to authorize an advance afterwards.
        assert_eq!(run.current_step_token, None);
        // A leftover pending inject must not block a later run's bookkeeping.
        assert_eq!(run.inject_pending_since, None);
        // The cell the run died on is kept as history.
        assert_eq!(run.current_cell_id, "c10");
    }

    #[tokio::test]
    async fn stopping_stalled_run_closes_it() {
        let repo = repo_with(RunStatus::Stalled).await;
        let clock = FakeIdClock::new();

        let out = stop_run(
            &repo,
            &clock,
            StopRunInput {
                run_id: "r1".into(),
            },
        )
        .await
        .unwrap();

        assert!(out.was_in_flight);
        assert_eq!(
            repo.get("r1").await.unwrap().unwrap().status,
            RunStatus::Closed
        );
    }

    #[tokio::test]
    async fn stopping_disconnected_run_closes_it() {
        let repo = repo_with(RunStatus::Disconnected).await;
        let clock = FakeIdClock::new();

        let out = stop_run(
            &repo,
            &clock,
            StopRunInput {
                run_id: "r1".into(),
            },
        )
        .await
        .unwrap();

        assert!(out.was_in_flight);
        assert_eq!(
            repo.get("r1").await.unwrap().unwrap().status,
            RunStatus::Closed
        );
    }

    #[tokio::test]
    async fn stopping_done_run_is_a_no_op() {
        let repo = repo_with(RunStatus::Done).await;
        let clock = FakeIdClock::new();

        let out = stop_run(
            &repo,
            &clock,
            StopRunInput {
                run_id: "r1".into(),
            },
        )
        .await
        .unwrap();

        assert!(!out.was_in_flight);
        // A completed run must not be rewritten into Closed.
        assert_eq!(
            repo.get("r1").await.unwrap().unwrap().status,
            RunStatus::Done
        );
    }

    #[tokio::test]
    async fn stopping_already_closed_run_is_idempotent() {
        let repo = repo_with(RunStatus::Closed).await;
        let clock = FakeIdClock::new();

        let out = stop_run(
            &repo,
            &clock,
            StopRunInput {
                run_id: "r1".into(),
            },
        )
        .await
        .unwrap();

        assert!(!out.was_in_flight);
        assert_eq!(
            repo.get("r1").await.unwrap().unwrap().status,
            RunStatus::Closed
        );
    }

    #[tokio::test]
    async fn stopping_missing_run_is_not_found() {
        let repo = InMemoryRunRepository::new();
        let clock = FakeIdClock::new();

        let err = stop_run(
            &repo,
            &clock,
            StopRunInput {
                run_id: "nope".into(),
            },
        )
        .await
        .unwrap_err();

        assert!(matches!(err, CoreError::NotFound(_)));
    }
}
