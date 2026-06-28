//! Output port for run persistence.

use crate::domain::run::Run;
use crate::error::CoreError;
use async_trait::async_trait;

#[async_trait]
pub trait RunRepository: Send + Sync {
    /// Persist a newly created run.
    async fn create(&self, run: &Run) -> Result<(), CoreError>;
    /// Fetch a run by id. Returns `Ok(None)` when not found.
    async fn get(&self, run_id: &str) -> Result<Option<Run>, CoreError>;
    /// Overwrite the mutable fields of an existing run (status, current_cell_id, updated_at).
    async fn update(&self, run: &Run) -> Result<(), CoreError>;
    /// List all runs for a given harness, newest first.
    async fn list_by_harness(&self, harness_id: &str) -> Result<Vec<Run>, CoreError>;
    /// Record a heartbeat timestamp for a run. No-op (Ok) if the run does not exist.
    async fn update_heartbeat(&self, run_id: &str, ts: &str) -> Result<(), CoreError>;
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

        async fn update(&self, run: &Run) -> Result<(), CoreError> {
            let mut map = self.runs.lock().unwrap();
            if !map.contains_key(&run.id) {
                return Err(CoreError::NotFound(run.id.clone()));
            }
            map.insert(run.id.clone(), run.clone());
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
    }
}
