use super::schema::SCHEMA;
use async_trait::async_trait;
use rusqlite::{Connection, OptionalExtension};
use std::sync::Mutex;
use sugo_core::domain::board::BoardDefinition;
use sugo_core::domain::harness::{BoardVersion, Harness};
use sugo_core::error::CoreError;
use sugo_core::ports::repository::HarnessRepository;

/// SQLite-backed [`HarnessRepository`].
///
/// Persists harness heads (`harnesses`) and immutable board-version snapshots
/// (`board_versions`), preserving per-harness monotonic `version_no` and
/// enforcing optimistic locking on edits. The single owned [`Connection`] is
/// guarded by a [`Mutex`]; each access recovers from poisoning so a panic while
/// the lock is held does not permanently brick the repository.
///
/// On construction the connection enables `PRAGMA foreign_keys = ON` (so the
/// `board_versions.harness_id` foreign key is enforced) and a `busy_timeout`,
/// and file-backed connections additionally switch to WAL journaling to support
/// the DB-as-authority multi-process coordination of the design.
pub struct SqliteHarnessRepository {
    conn: Mutex<Connection>,
}

impl SqliteHarnessRepository {
    /// Opens (creating if absent) a file-backed repository at `path`.
    ///
    /// Applies the schema, enables foreign-key enforcement and a busy timeout,
    /// and switches the database to WAL journaling for concurrent access.
    pub fn open(path: &str) -> Result<Self, CoreError> {
        let conn = Connection::open(path).map_err(map_err)?;
        Self::init(conn, true)
    }

    /// Opens an ephemeral in-memory repository.
    ///
    /// Applies the schema, enables foreign-key enforcement and a busy timeout.
    /// WAL journaling is not applied because it is meaningless for `:memory:`
    /// databases. Primarily intended for tests.
    pub fn in_memory() -> Result<Self, CoreError> {
        let conn = Connection::open_in_memory().map_err(map_err)?;
        Self::init(conn, false)
    }

    fn init(conn: Connection, file_backed: bool) -> Result<Self, CoreError> {
        // Enforce foreign keys for this connection. SQLite leaves FK
        // enforcement off by default, so without this the
        // board_versions.harness_id REFERENCES clause is purely advisory.
        conn.pragma_update(None, "foreign_keys", true)
            .map_err(map_err)?;
        // Wait (with retry) instead of failing immediately on SQLITE_BUSY, so
        // that the DB-as-authority multi-process coordination can make progress
        // under contention rather than erroring out.
        conn.pragma_update(None, "busy_timeout", 5000)
            .map_err(map_err)?;
        if file_backed {
            // WAL improves reader/writer concurrency for file-backed databases.
            // It is meaningless for :memory: connections (they stay "memory"),
            // so it is only attempted here.
            conn.pragma_update(None, "journal_mode", "WAL")
                .map_err(map_err)?;
        }
        conn.execute_batch(SCHEMA).map_err(map_err)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Locks the connection, recovering from a poisoned mutex.
    ///
    /// If a previous holder panicked while the lock was held the mutex is
    /// poisoned; we take the inner guard anyway rather than propagating the
    /// panic, so a single failed operation does not brick every later call.
    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn map_err(e: rusqlite::Error) -> CoreError {
    CoreError::Storage(e.to_string())
}

#[async_trait]
impl HarnessRepository for SqliteHarnessRepository {
    async fn create(&self, harness: &Harness, version: &BoardVersion) -> Result<(), CoreError> {
        let mut conn = self.lock();
        // Wrap INSERT harnesses + INSERT board_version in one transaction so a
        // mid-way failure leaves no partial commit (rolled back).
        let tx = conn.transaction().map_err(map_err)?;
        insert_harness(&tx, harness)?;
        insert_version(&tx, version)?;
        tx.commit().map_err(map_err)?;
        Ok(())
    }

    async fn get(&self, id: &str) -> Result<Option<(Harness, BoardVersion)>, CoreError> {
        let conn = self.lock();
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
        let conn = self.lock();
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
        let conn = self.lock();
        select_version(&conn, harness_id, version_no)
    }

    async fn append_version(
        &self,
        harness: &Harness,
        version: &BoardVersion,
        expected_lock: i64,
    ) -> Result<(), CoreError> {
        let mut conn = self.lock();
        // Wrap "read lock_version -> compare -> INSERT board_version ->
        // UPDATE harnesses" in one transaction. A mid-way failure is rolled
        // back instead of committed, preventing an inconsistency where only the
        // board_version is left behind.
        let tx = conn.transaction().map_err(map_err)?;

        // Pre-SELECT lets us distinguish NotFound from LockConflict in the
        // returned error.
        let current = select_harness(&tx, &harness.id)?
            .ok_or_else(|| CoreError::NotFound(harness.id.clone()))?;
        if current.lock_version != expected_lock {
            return Err(CoreError::LockConflict {
                expected: expected_lock,
                actual: current.lock_version,
            });
        }

        // Append the immutable board_version; UNIQUE(harness_id, version_no)
        // rejects duplicates.
        insert_version(&tx, version)?;

        // CAS-guarded UPDATE. The WHERE id=? AND lock_version=?expected clause
        // updates atomically at the DB level; zero rows affected (i.e. the
        // expected lock no longer matches) maps to LockConflict.
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
