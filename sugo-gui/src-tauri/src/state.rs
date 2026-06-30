use std::sync::{Arc, Mutex};
use sugo_infra::sqlite::repository::SqliteHarnessRepository;
use sugo_infra::sqlite::run_repository::SqliteRunRepository;

pub struct AppState {
    pub repo: Arc<SqliteHarnessRepository>,
    pub run_repo: Arc<SqliteRunRepository>,
}

impl AppState {
    pub fn new(db_path: &str) -> Result<Self, String> {
        let repo = SqliteHarnessRepository::open(db_path).map_err(|e| e.to_string())?;
        let conn = rusqlite::Connection::open(db_path).map_err(|e| e.to_string())?;
        let run_repo = SqliteRunRepository::new(Mutex::new(conn));
        Ok(Self {
            repo: Arc::new(repo),
            run_repo: Arc::new(run_repo),
        })
    }
}
