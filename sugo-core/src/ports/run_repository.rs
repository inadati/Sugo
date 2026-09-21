//! Output port for run persistence.

use crate::domain::run::{Run, RunStatus};
use crate::error::CoreError;
use async_trait::async_trait;

#[async_trait]
pub trait RunRepository: Send + Sync {
    /// Persist a newly created run.
    async fn create(&self, run: &Run) -> Result<(), CoreError>;
    /// Fetch a run by id. Returns `Ok(None)` when not found.
    async fn get(&self, run_id: &str) -> Result<Option<Run>, CoreError>;
    /// Move a run to `cell_id` with `status`, stamping `updated_at`.
    ///
    /// Writes exactly those three fields. Returns [`CoreError::NotFound`] when
    /// the run does not exist.
    async fn set_position(
        &self,
        run_id: &str,
        cell_id: &str,
        status: RunStatus,
        updated_at: &str,
    ) -> Result<(), CoreError>;
    /// Set a run's `status`, stamping `updated_at`, leaving its position intact.
    ///
    /// Writes exactly those two fields. Returns [`CoreError::NotFound`] when
    /// the run does not exist.
    async fn set_status(
        &self,
        run_id: &str,
        status: RunStatus,
        updated_at: &str,
    ) -> Result<(), CoreError>;
    /// List all runs for a given harness, newest first.
    async fn list_by_harness(&self, harness_id: &str) -> Result<Vec<Run>, CoreError>;
    /// Record a heartbeat timestamp for a run. No-op (Ok) if the run does not exist.
    async fn update_heartbeat(&self, run_id: &str, ts: &str) -> Result<(), CoreError>;
    /// Set or clear the inject_pending_since timestamp. Pass Some(ts) when an inject is sent,
    /// None to clear (inject acknowledged by Nipper). No-op if run does not exist.
    async fn set_inject_pending(&self, run_id: &str, ts: Option<&str>) -> Result<(), CoreError>;
    /// Set or clear the one-time step token. Pass Some(token) just before an inject is sent,
    /// None once the advance it authorizes has succeeded. No-op if run does not exist.
    async fn set_step_token(&self, run_id: &str, token: Option<&str>) -> Result<(), CoreError>;
}

#[cfg(any(test, feature = "test-support"))]
pub mod fake {
    use super::*;
    use crate::domain::run::Run;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    pub struct InMemoryRunRepository {
        runs: Mutex<HashMap<String, Run>>,
    }

    impl InMemoryRunRepository {
        pub fn new() -> Self {
            Self::default()
        }
    }

    #[async_trait]
    impl RunRepository for InMemoryRunRepository {
        async fn create(&self, run: &Run) -> Result<(), CoreError> {
            let mut map = self.runs.lock().unwrap();
            if map.contains_key(&run.id) {
                return Err(CoreError::Storage(format!("duplicate run id: {}", run.id)));
            }
            map.insert(run.id.clone(), run.clone());
            Ok(())
        }

        async fn get(&self, run_id: &str) -> Result<Option<Run>, CoreError> {
            Ok(self.runs.lock().unwrap().get(run_id).cloned())
        }

        async fn set_position(
            &self,
            run_id: &str,
            cell_id: &str,
            status: RunStatus,
            updated_at: &str,
        ) -> Result<(), CoreError> {
            let mut map = self.runs.lock().unwrap();
            let Some(stored) = map.get_mut(run_id) else {
                return Err(CoreError::NotFound(run_id.to_string()));
            };
            stored.current_cell_id = cell_id.to_string();
            stored.status = status;
            stored.updated_at = updated_at.to_string();
            Ok(())
        }

        async fn set_status(
            &self,
            run_id: &str,
            status: RunStatus,
            updated_at: &str,
        ) -> Result<(), CoreError> {
            let mut map = self.runs.lock().unwrap();
            let Some(stored) = map.get_mut(run_id) else {
                return Err(CoreError::NotFound(run_id.to_string()));
            };
            stored.status = status;
            stored.updated_at = updated_at.to_string();
            Ok(())
        }

        async fn list_by_harness(&self, harness_id: &str) -> Result<Vec<Run>, CoreError> {
            let map = self.runs.lock().unwrap();
            let mut runs: Vec<Run> = map
                .values()
                .filter(|r| r.harness_id == harness_id)
                .cloned()
                .collect();
            runs.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            Ok(runs)
        }

        async fn update_heartbeat(&self, run_id: &str, ts: &str) -> Result<(), CoreError> {
            let mut map = self.runs.lock().unwrap();
            if let Some(run) = map.get_mut(run_id) {
                run.last_heartbeat_at = Some(ts.to_string());
            }
            Ok(())
        }

        async fn set_inject_pending(
            &self,
            run_id: &str,
            ts: Option<&str>,
        ) -> Result<(), CoreError> {
            let mut map = self.runs.lock().unwrap();
            if let Some(run) = map.get_mut(run_id) {
                run.inject_pending_since = ts.map(|s| s.to_string());
            }
            Ok(())
        }

        async fn set_step_token(&self, run_id: &str, token: Option<&str>) -> Result<(), CoreError> {
            let mut map = self.runs.lock().unwrap();
            if let Some(run) = map.get_mut(run_id) {
                run.current_step_token = token.map(|s| s.to_string());
            }
            Ok(())
        }
    }
}
