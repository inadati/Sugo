//! SQLite-backed CellPositionRepository.
//!
//! 座標は表示上の情報であり、`board_versions` の不変 JSON とは独立に
//! 保持される。版は上がらず、楽観ロックも掛けない（後勝ち）。

use async_trait::async_trait;
use std::sync::Mutex;
use sugo_core::domain::cell_position::CellPosition;
use sugo_core::error::CoreError;
use sugo_core::ports::cell_position_repository::CellPositionRepository;

fn map_err(e: rusqlite::Error) -> CoreError {
    CoreError::Storage(e.to_string())
}

pub struct SqliteCellPositionRepository {
    conn: Mutex<rusqlite::Connection>,
}

impl SqliteCellPositionRepository {
    pub fn new(conn: Mutex<rusqlite::Connection>) -> Self {
        Self { conn }
    }
}

#[async_trait]
impl CellPositionRepository for SqliteCellPositionRepository {
    async fn list(&self, harness_id: &str) -> Result<Vec<CellPosition>, CoreError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn
            .prepare("SELECT cell_id, x, y FROM cell_positions WHERE harness_id = ?1")
            .map_err(map_err)?;
        let rows = stmt
            .query_map([harness_id], |row| {
                Ok(CellPosition {
                    cell_id: row.get(0)?,
                    x: row.get(1)?,
                    y: row.get(2)?,
                })
            })
            .map_err(map_err)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(map_err)?);
        }
        Ok(out)
    }

    async fn replace_all(
        &self,
        harness_id: &str,
        positions: &[CellPosition],
    ) -> Result<(), CoreError> {
        let mut conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let tx = conn.transaction().map_err(map_err)?;
        tx.execute("DELETE FROM cell_positions WHERE harness_id = ?1", [harness_id])
            .map_err(map_err)?;
        for p in positions {
            tx.execute(
                "INSERT INTO cell_positions (harness_id, cell_id, x, y) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![harness_id, p.cell_id, p.x, p.y],
            )
            .map_err(map_err)?;
        }
        tx.commit().map_err(map_err)?;
        Ok(())
    }

    async fn upsert(
        &self,
        harness_id: &str,
        positions: &[CellPosition],
    ) -> Result<(), CoreError> {
        let mut conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let tx = conn.transaction().map_err(map_err)?;
        for p in positions {
            tx.execute(
                "INSERT INTO cell_positions (harness_id, cell_id, x, y) VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT(harness_id, cell_id) DO UPDATE SET x = ?3, y = ?4",
                rusqlite::params![harness_id, p.cell_id, p.x, p.y],
            )
            .map_err(map_err)?;
        }
        tx.commit().map_err(map_err)?;
        Ok(())
    }

    async fn clear(&self, harness_id: &str) -> Result<(), CoreError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute("DELETE FROM cell_positions WHERE harness_id = ?1", [harness_id])
            .map_err(map_err)?;
        Ok(())
    }
}
