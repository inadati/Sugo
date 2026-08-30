//! SQLite-backed RunRepository.

use async_trait::async_trait;
use rusqlite::OptionalExtension;
use std::sync::Mutex;
use sugo_core::domain::run::{Run, RunStatus};
use sugo_core::error::CoreError;
use sugo_core::ports::run_repository::RunRepository;

fn parse_status(s: &str) -> RunStatus {
    match s {
        "done" => RunStatus::Done,
        "stalled" => RunStatus::Stalled,
        "disconnected" => RunStatus::Disconnected,
        "closed" => RunStatus::Closed,
        _ => RunStatus::Running,
    }
}

fn status_str(s: &RunStatus) -> &'static str {
    match s {
        RunStatus::Running => "running",
        RunStatus::Done => "done",
        RunStatus::Stalled => "stalled",
        RunStatus::Disconnected => "disconnected",
        RunStatus::Closed => "closed",
    }
}

fn map_err(e: rusqlite::Error) -> CoreError {
    CoreError::Storage(e.to_string())
}

pub struct SqliteRunRepository {
    conn: Mutex<rusqlite::Connection>,
}

impl SqliteRunRepository {
    pub fn new(conn: Mutex<rusqlite::Connection>) -> Self {
        Self { conn }
    }
}

#[async_trait]
impl RunRepository for SqliteRunRepository {
    async fn create(&self, run: &Run) -> Result<(), CoreError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "INSERT INTO runs (id, harness_id, board_version_no, current_cell_id, status, project_path, created_at, updated_at, last_heartbeat_at, inject_pending_since)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                run.id, run.harness_id, run.board_version_no, run.current_cell_id,
                status_str(&run.status), run.project_path, run.created_at, run.updated_at,
                run.last_heartbeat_at, run.inject_pending_since
            ],
        )
        .map_err(map_err)?;
        Ok(())
    }

    async fn get(&self, run_id: &str) -> Result<Option<Run>, CoreError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let row = conn
            .query_row(
                "SELECT id, harness_id, board_version_no, current_cell_id, status, project_path, created_at, updated_at, last_heartbeat_at, inject_pending_since \
                 FROM runs WHERE id = ?1",
                [run_id],
                |row| {
                    Ok(Run {
                        id: row.get(0)?,
                        harness_id: row.get(1)?,
                        board_version_no: row.get(2)?,
                        current_cell_id: row.get(3)?,
                        status: {
                            let s: String = row.get(4)?;
                            parse_status(&s)
                        },
                        project_path: row.get(5)?,
                        created_at: row.get(6)?,
                        updated_at: row.get(7)?,
                        last_heartbeat_at: row.get(8)?,
                        inject_pending_since: row.get(9)?,
                    })
                },
            )
            .optional()
            .map_err(map_err)?;
        Ok(row)
    }

    async fn update(&self, run: &Run) -> Result<(), CoreError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let n = conn
            .execute(
                "UPDATE runs SET current_cell_id = ?1, status = ?2, updated_at = ?3 WHERE id = ?4",
                rusqlite::params![
                    run.current_cell_id,
                    status_str(&run.status),
                    run.updated_at,
                    run.id
                ],
            )
            .map_err(map_err)?;
        if n == 0 {
            return Err(CoreError::NotFound(run.id.clone()));
        }
        Ok(())
    }

    async fn list_by_harness(&self, harness_id: &str) -> Result<Vec<Run>, CoreError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn
            .prepare(
                "SELECT id, harness_id, board_version_no, current_cell_id, status, project_path, created_at, updated_at, last_heartbeat_at, inject_pending_since \
                 FROM runs WHERE harness_id = ?1 ORDER BY created_at DESC",
            )
            .map_err(map_err)?;
        let rows = stmt
            .query_map([harness_id], |row| {
                Ok(Run {
                    id: row.get(0)?,
                    harness_id: row.get(1)?,
                    board_version_no: row.get(2)?,
                    current_cell_id: row.get(3)?,
                    status: {
                        let s: String = row.get(4)?;
                        parse_status(&s)
                    },
                    project_path: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                    last_heartbeat_at: row.get(8)?,
                    inject_pending_since: row.get(9)?,
                })
            })
            .map_err(map_err)?;
        rows.map(|r| r.map_err(map_err)).collect()
    }

    async fn update_heartbeat(&self, run_id: &str, ts: &str) -> Result<(), CoreError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "UPDATE runs SET last_heartbeat_at = ?1 WHERE id = ?2",
            rusqlite::params![ts, run_id],
        )
        .map_err(map_err)?;
        Ok(())
    }

    async fn set_inject_pending(&self, run_id: &str, ts: Option<&str>) -> Result<(), CoreError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "UPDATE runs SET inject_pending_since = ?1 WHERE id = ?2",
            rusqlite::params![ts, run_id],
        )
        .map_err(map_err)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::schema::SCHEMA;
    use rusqlite::Connection;
    use sugo_core::domain::run::RunStatus;

    fn repo() -> SqliteRunRepository {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        SqliteRunRepository::new(Mutex::new(conn))
    }

    fn sample_run(id: &str) -> Run {
        Run {
            id: id.into(),
            harness_id: "h1".into(),
            board_version_no: 1,
            current_cell_id: "c1".into(),
            status: RunStatus::Running,
            project_path: None,
            created_at: "2026-01-01T00:00:00+09:00".into(),
            updated_at: "2026-01-01T00:00:00+09:00".into(),
            last_heartbeat_at: None,
            inject_pending_since: None,
        }
    }

    #[tokio::test]
    async fn create_and_get_run() {
        let r = repo();
        let run = sample_run("r1");
        r.create(&run).await.unwrap();
        let got = r.get("r1").await.unwrap().unwrap();
        assert_eq!(got.id, "r1");
        assert_eq!(got.status, RunStatus::Running);
        assert_eq!(got.current_cell_id, "c1");
    }

    #[tokio::test]
    async fn get_missing_run_returns_none() {
        let r = repo();
        let got = r.get("nope").await.unwrap();
        assert!(got.is_none());
    }

    #[tokio::test]
    async fn update_run_persists_status_and_cell() {
        let r = repo();
        let mut run = sample_run("r1");
        r.create(&run).await.unwrap();
        run.current_cell_id = "c2".into();
        run.status = RunStatus::Done;
        run.updated_at = "2026-06-01T00:00:00+09:00".into();
        r.update(&run).await.unwrap();
        let got = r.get("r1").await.unwrap().unwrap();
        assert_eq!(got.current_cell_id, "c2");
        assert_eq!(got.status, RunStatus::Done);
    }

    #[tokio::test]
    async fn list_by_harness_returns_runs() {
        let r = repo();
        r.create(&sample_run("r1")).await.unwrap();
        let mut r2 = sample_run("r2");
        r2.harness_id = "h2".into();
        r.create(&r2).await.unwrap();
        let list = r.list_by_harness("h1").await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "r1");
    }

    #[tokio::test]
    async fn update_heartbeat_persists_timestamp() {
        let r = repo();
        r.create(&sample_run("r1")).await.unwrap();
        r.update_heartbeat("r1", "2026-06-28T12:00:00+09:00")
            .await
            .unwrap();
        let got = r.get("r1").await.unwrap().unwrap();
        assert_eq!(
            got.last_heartbeat_at.as_deref(),
            Some("2026-06-28T12:00:00+09:00")
        );
    }

    #[tokio::test]
    async fn update_heartbeat_unknown_run_is_ok() {
        let r = repo();
        assert!(
            r.update_heartbeat("ghost", "2026-06-28T12:00:00+09:00")
                .await
                .is_ok()
        );
    }
}
