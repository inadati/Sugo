use super::schema::SCHEMA;
use async_trait::async_trait;
use rusqlite::{Connection, OptionalExtension};
use std::sync::Mutex;
use sugo_core::domain::board::BoardDefinition;
use sugo_core::domain::harness::{BoardVersion, Harness};
use sugo_core::error::CoreError;
use sugo_core::ports::repository::HarnessRepository;

pub struct SqliteHarnessRepository {
    conn: Mutex<Connection>,
}

impl SqliteHarnessRepository {
    pub fn open(path: &str) -> Result<Self, CoreError> {
        let conn = Connection::open(path).map_err(map_err)?;
        Self::init(conn)
    }

    pub fn in_memory() -> Result<Self, CoreError> {
        let conn = Connection::open_in_memory().map_err(map_err)?;
        Self::init(conn)
    }

    fn init(conn: Connection) -> Result<Self, CoreError> {
        conn.execute_batch(SCHEMA).map_err(map_err)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

fn map_err(e: rusqlite::Error) -> CoreError {
    CoreError::Storage(e.to_string())
}

#[async_trait]
impl HarnessRepository for SqliteHarnessRepository {
    async fn create(&self, harness: &Harness, version: &BoardVersion) -> Result<(), CoreError> {
        let mut conn = self.conn.lock().unwrap();
        // INSERT harnesses + INSERT board_version を1トランザクションにまとめ、
        // 途中失敗時に部分コミットを残さない（rollback）。
        let tx = conn.transaction().map_err(map_err)?;
        insert_harness(&tx, harness)?;
        insert_version(&tx, version)?;
        tx.commit().map_err(map_err)?;
        Ok(())
    }

    async fn get(&self, id: &str) -> Result<Option<(Harness, BoardVersion)>, CoreError> {
        let conn = self.conn.lock().unwrap();
        let harness = select_harness(&conn, id)?;
        match harness {
            None => Ok(None),
            Some(h) => {
                let v = select_version(&conn, id, h.current_version)?
                    .ok_or_else(|| CoreError::Storage("missing head version".into()))?;
                Ok(Some((h, v)))
            }
        }
    }

    async fn list(&self) -> Result<Vec<Harness>, CoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT id,name,current_version,has_draft,lock_version,created_at,updated_at FROM harnesses",
            )
            .map_err(map_err)?;
        let rows = stmt
            .query_map([], row_to_harness)
            .map_err(map_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_err)?;
        Ok(rows)
    }

    async fn get_version(
        &self,
        harness_id: &str,
        version_no: i64,
    ) -> Result<Option<BoardVersion>, CoreError> {
        let conn = self.conn.lock().unwrap();
        select_version(&conn, harness_id, version_no)
    }

    async fn append_version(
        &self,
        harness: &Harness,
        version: &BoardVersion,
        expected_lock: i64,
    ) -> Result<(), CoreError> {
        let mut conn = self.conn.lock().unwrap();
        // 「lock_version 読取 → 比較 → INSERT board_version → UPDATE harnesses」を
        // 1トランザクションにまとめる。途中失敗時は commit せず rollback され、
        // board_version だけ残る不整合を防ぐ。
        let tx = conn.transaction().map_err(map_err)?;

        // 事前 SELECT で NotFound / LockConflict を区別して返す。
        let current = select_harness(&tx, &harness.id)?
            .ok_or_else(|| CoreError::NotFound(harness.id.clone()))?;
        if current.lock_version != expected_lock {
            return Err(CoreError::LockConflict {
                expected: expected_lock,
                actual: current.lock_version,
            });
        }

        // 不変 board_version を追加（UNIQUE(harness_id, version_no) で重複拒否）。
        insert_version(&tx, version)?;

        // CAS ガード付き UPDATE。WHERE id=? AND lock_version=?expected により
        // DBレベルで原子的に更新し、0 行更新（= 期待ロック不一致）なら LockConflict。
        let affected = tx
            .execute(
                "UPDATE harnesses SET current_version=?1, has_draft=?2, lock_version=?3, updated_at=?4 WHERE id=?5 AND lock_version=?6",
                rusqlite::params![
                    harness.current_version,
                    harness.has_draft as i64,
                    harness.lock_version,
                    harness.updated_at,
                    harness.id,
                    expected_lock
                ],
            )
            .map_err(map_err)?;
        if affected == 0 {
            return Err(CoreError::LockConflict {
                expected: expected_lock,
                actual: current.lock_version,
            });
        }

        tx.commit().map_err(map_err)?;
        Ok(())
    }
}

fn insert_harness(conn: &Connection, h: &Harness) -> Result<(), CoreError> {
    conn.execute(
        "INSERT INTO harnesses (id,name,current_version,has_draft,lock_version,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
        rusqlite::params![
            h.id,
            h.name,
            h.current_version,
            h.has_draft as i64,
            h.lock_version,
            h.created_at,
            h.updated_at
        ],
    )
    .map_err(map_err)?;
    Ok(())
}

fn insert_version(conn: &Connection, v: &BoardVersion) -> Result<(), CoreError> {
    let json = serde_json::to_string(&v.definition).map_err(|e| CoreError::Storage(e.to_string()))?;
    conn.execute(
        "INSERT INTO board_versions (id,harness_id,version_no,definition_json,content_hash,created_at) VALUES (?1,?2,?3,?4,?5,?6)",
        rusqlite::params![v.id, v.harness_id, v.version_no, json, v.content_hash, v.created_at],
    )
    .map_err(map_err)?;
    Ok(())
}

fn row_to_harness(row: &rusqlite::Row) -> rusqlite::Result<Harness> {
    Ok(Harness {
        id: row.get(0)?,
        name: row.get(1)?,
        current_version: row.get(2)?,
        has_draft: row.get::<_, i64>(3)? != 0,
        lock_version: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn select_harness(conn: &Connection, id: &str) -> Result<Option<Harness>, CoreError> {
    conn.query_row(
        "SELECT id,name,current_version,has_draft,lock_version,created_at,updated_at FROM harnesses WHERE id=?1",
        [id],
        row_to_harness,
    )
    .optional()
    .map_err(map_err)
}

fn select_version(
    conn: &Connection,
    harness_id: &str,
    version_no: i64,
) -> Result<Option<BoardVersion>, CoreError> {
    conn.query_row(
        "SELECT id,harness_id,version_no,definition_json,content_hash,created_at FROM board_versions WHERE harness_id=?1 AND version_no=?2",
        rusqlite::params![harness_id, version_no],
        |row| {
            let json: String = row.get(3)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                json,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        },
    )
    .optional()
    .map_err(map_err)?
    .map(|(id, harness_id, version_no, json, content_hash, created_at)| {
        let definition: BoardDefinition =
            serde_json::from_str(&json).map_err(|e| CoreError::Storage(e.to_string()))?;
        Ok(BoardVersion {
            id,
            harness_id,
            version_no,
            definition,
            content_hash,
            created_at,
        })
    })
    .transpose()
}
