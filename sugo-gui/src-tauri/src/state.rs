use std::sync::Arc;
use sugo_infra::sqlite::repository::SqliteHarnessRepository;

pub struct AppState {
    pub repo: Arc<SqliteHarnessRepository>,
}

impl AppState {
    pub fn new(db_path: &str) -> Result<Self, String> {
        let repo = SqliteHarnessRepository::open(db_path).map_err(|e| e.to_string())?;
        Ok(Self { repo: Arc::new(repo) })
    }
}
