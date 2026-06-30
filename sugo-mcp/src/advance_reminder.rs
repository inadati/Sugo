//! Background task: after Nipper acks a prompt inject, monitor the project
//! jsonl for a "result" entry (= Claude Code turn complete). If sugo_advance
//! has not been called within the polling window, inject a reminder into Nipper.

use std::sync::Arc;
use sugo_core::domain::run::RunStatus;
use sugo_core::ports::run_repository::RunRepository;
use sugo_infra::jsonl_watcher;
use sugo_infra::sqlite::SqliteRunRepository;

const POLL_SECS: u64 = 5;
const MAX_REMINDERS: u32 = 3;

/// Spawn a background task that reminds the agent to call `sugo_advance`
/// if the jsonl shows turn-complete but advance has not been called.
pub fn spawn(
    run_id: String,
    project_path: String,
    run_repo: Arc<SqliteRunRepository>,
    nipper_base: String,
    since_iso: String,
) {
    tokio::spawn(async move {
        let mut reminders_sent: u32 = 0;

        loop {
            tokio::time::sleep(std::time::Duration::from_secs(POLL_SECS)).await;

            let Ok(Some(run)) = run_repo.get(&run_id).await else { break };
            if run.status != RunStatus::Running { break }
            // advance was called (new inject in flight) — stop watching
            if run.inject_pending_since.is_some() { break }

            // Turn not yet complete — keep waiting
            if !jsonl_watcher::has_assistant_entry_since(&project_path, &since_iso) { continue }

            if reminders_sent >= MAX_REMINDERS { break }

            let msg = format!(
                "【Sugo】Claude Code のターンが完了しましたが `sugo_advance` がまだ呼ばれていません。\n\
                 このセルのタスクが完了したら `sugo_advance` を呼び出して次のセルに進んでください。\n\
                 run_id: `{run_id}`"
            );
            let _ = crate::nipper_client::inject(&nipper_base, &project_path, &msg).await;
            reminders_sent += 1;
        }
    });
}
