use std::sync::{Arc, Mutex};
use sugo_infra::sqlite::cell_position_repository::SqliteCellPositionRepository;
use sugo_infra::sqlite::repository::SqliteHarnessRepository;
use sugo_infra::sqlite::run_repository::SqliteRunRepository;

pub struct AppState {
    pub repo: Arc<SqliteHarnessRepository>,
    pub run_repo: Arc<SqliteRunRepository>,
    pub pos_repo: Arc<SqliteCellPositionRepository>,
}

impl AppState {
    pub fn new(db_path: &str) -> Result<Self, String> {
        let repo = SqliteHarnessRepository::open(db_path).map_err(|e| e.to_string())?;
        let conn = rusqlite::Connection::open(db_path).map_err(|e| e.to_string())?;
        let run_repo = SqliteRunRepository::new(Mutex::new(conn));
        // 座標用にもう1本コネクションを開く。run_repo と同じく
        // スキーマ適用は SqliteHarnessRepository::open が済ませている。
        let pos_conn = rusqlite::Connection::open(db_path).map_err(|e| e.to_string())?;
        let pos_repo = SqliteCellPositionRepository::new(Mutex::new(pos_conn));
        Ok(Self {
            repo: Arc::new(repo),
            run_repo: Arc::new(run_repo),
            pos_repo: Arc::new(pos_repo),
        })
    }
}
