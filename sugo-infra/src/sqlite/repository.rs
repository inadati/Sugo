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
            //
            // `journal_mode` is a special PRAGMA: SQLite reports the resulting
            // mode in a result row, and on some filesystems (e.g. networked FS)
            // the switch can silently fall back to another mode without erroring.
            // `pragma_update` discards that row, so we read the mode back and
            // fail loudly if WAL was not actually applied, rather than running on
            // an unexpected journal mode.
            let mode: String = conn
                .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
                .map_err(map_err)?;
            if !mode.eq_ignore_ascii_case("wal") {
                return Err(CoreError::Storage(format!(
                    "expected WAL journal mode, got '{mode}'"
                )));
            }
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

#[cfg(test)]
mod tests {
    //! Connection-level unit tests that need direct access to the repository's
    //! private `Connection` (PRAGMA state, raw SQL, the poison-recovering
    //! `lock()`). Behaviour reachable through the public port lives in the
    //! integration suite (`tests/sqlite_repository.rs`); these cover the wiring
    //! that the public API alone cannot exercise: that `foreign_keys=ON` is
    //! actually applied, that the WAL branch matches `file_backed`, that
    //! `busy_timeout` absorbs cross-connection write contention, and that a
    //! poisoned mutex still yields a usable connection.
    use super::*;
    use sugo_core::domain::cell::{Cell, CellStatus};

    /// Minimal valid board: one active terminal cell carrying `prompt`.
    fn board(prompt: &str) -> BoardDefinition {
        BoardDefinition {
            schema_version: 1,
            start: "c1".into(),
            cells: vec![Cell {
                id: "c1".into(),
                name: "c1".into(),
                prompt: prompt.into(),
                status: CellStatus::Active,
                terminal: true,
            }],
            edges: vec![],
        }
    }

    fn harness(id: &str, current_version: i64, lock_version: i64) -> Harness {
        Harness {
            id: id.into(),
            name: "h".into(),
            current_version,
            has_draft: false,
            lock_version,
            created_at: "t".into(),
            updated_at: "t".into(),
        }
    }

    fn version(id: &str, harness_id: &str, version_no: i64, prompt: &str) -> BoardVersion {
        BoardVersion {
            id: id.into(),
            harness_id: harness_id.into(),
            version_no,
            definition: board(prompt),
            content_hash: "hash".into(),
            created_at: "t".into(),
        }
    }

    /// A fresh temp directory unique to this process, `tag`, *and* this call.
    ///
    /// The name combines the PID, a wall-clock nanosecond stamp, and a
    /// process-wide monotonic counter (`sugo-{tag}-{pid}-{nanos}-{counter}`).
    /// The PID alone is not enough: PIDs are reused (CI wrappers, macOS), so a
    /// prior run that panicked/aborted before its best-effort `remove_dir_all`
    /// could leave a seeded DB behind at the same path, and the next run's
    /// seeding `create()` would then collide on `harnesses.id`'s PRIMARY KEY.
    /// The nanosecond stamp defeats cross-run PID reuse and the atomic counter
    /// defeats same-process same-tag reuse, so each returned directory is unique
    /// and can never alias another test's or a past run's residue. The directory
    /// is (re)created empty so file-backed tests always start from a clean slate.
    fn temp_dir(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::time::{SystemTime, UNIX_EPOCH};

        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("sugo-{tag}-{pid}-{nanos}-{counter}"));
        // Defensive: should never already exist given the unique name, but if a
        // path somehow aliased one, start from empty rather than inherit residue.
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    /// Poisoning the connection mutex (by panicking while the guard is held)
    /// must not permanently brick the repository: a later `lock()` recovers the
    /// inner guard and a real query through the recovered connection succeeds.
    #[test]
    fn lock_recovers_from_poisoned_mutex() {
        let repo = SqliteHarnessRepository::in_memory().expect("in-memory repo");

        // Panic while holding the lock to poison the mutex.
        let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = repo.conn.lock().expect("first lock is not yet poisoned");
            panic!("intentional panic while holding the connection lock");
        }));
        assert!(unwound.is_err(), "the held-lock panic must unwind");
        assert!(
            repo.conn.is_poisoned(),
            "the mutex must be poisoned after the panic"
        );

        // The recovering lock() must still hand back a usable connection.
        let guard = repo.lock();
        let count: i64 = guard
            .query_row("SELECT count(*) FROM harnesses", [], |row| row.get(0))
            .expect("query works on the recovered connection");
        assert_eq!(count, 0, "schema is intact on the recovered connection");
    }

    /// Regression guard for `PRAGMA foreign_keys = ON`: a direct INSERT of an
    /// orphan `board_versions` row (a `harness_id` with no parent) bypasses
    /// `append_version`'s application-level pre-SELECT and must be rejected by
    /// the FK constraint itself. If the pragma were silently removed this raw
    /// INSERT would succeed and this test would fail — which is the point.
    #[test]
    fn raw_orphan_board_version_insert_is_rejected_by_fk() {
        let repo = SqliteHarnessRepository::in_memory().expect("in-memory repo");
        let conn = repo.lock();
        let err = conn
            .execute(
                "INSERT INTO board_versions (id,harness_id,version_no,definition_json,content_hash,created_at) VALUES (?1,?2,?3,?4,?5,?6)",
                rusqlite::params!["orphan-id", "ghost-harness", 1, "{}", "hash", "t"],
            )
            .expect_err("orphan board_version INSERT must violate the harness_id FK");
        match err {
            rusqlite::Error::SqliteFailure(e, _) => assert_eq!(
                e.code,
                rusqlite::ErrorCode::ConstraintViolation,
                "expected an FK constraint violation, got {e:?}"
            ),
            other => panic!("expected a SqliteFailure constraint violation, got {other:?}"),
        }
    }

    /// The `journal_mode` branch must match `file_backed`: an `in_memory()`
    /// connection stays off WAL while an `open()` (file-backed) connection is
    /// actually switched to WAL. Asserted by querying `PRAGMA journal_mode`.
    #[test]
    fn journal_mode_matches_file_backed_branch() {
        let mem = SqliteHarnessRepository::in_memory().expect("in-memory repo");
        let mem_mode: String = mem
            .lock()
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .expect("read in-memory journal_mode");
        assert!(
            !mem_mode.eq_ignore_ascii_case("wal"),
            "in_memory must not use WAL, got '{mem_mode}'"
        );

        let dir = temp_dir("jm");
        let path = dir.join("jm.db");
        let repo = SqliteHarnessRepository::open(path.to_str().unwrap()).expect("open file repo");
        let file_mode: String = repo
            .lock()
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .expect("read file-backed journal_mode");
        assert!(
            file_mode.eq_ignore_ascii_case("wal"),
            "file-backed must use WAL, got '{file_mode}'"
        );
        drop(repo);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Multi-connection write contention: a second connection on the SAME file
    /// (standing in for another process) holds the write lock briefly. With
    /// `busy_timeout` set, a contended write from the repository must WAIT for
    /// the holder to release and then succeed, rather than failing immediately
    /// with `SQLITE_BUSY` ("database is locked"). A barrier channel makes the
    /// lock ordering deterministic.
    ///
    /// The contended operation is a fresh `create()` rather than an
    /// `append_version()`: `create()`'s deferred transaction issues a write as
    /// its first statement, so it requests the write lock from the start and
    /// `busy_timeout` applies. (An `append_version()` reads first, so its later
    /// INSERT is a read→write upgrade for which SQLite returns BUSY immediately
    /// and bypasses `busy_timeout` to avoid deadlock — the wrong path to test.)
    #[tokio::test]
    async fn busy_timeout_absorbs_cross_connection_write_contention() {
        use std::time::Duration;

        let dir = temp_dir("busy");
        let path = dir.join("busy.db");
        let path_str = path.to_str().unwrap().to_string();

        let repo = SqliteHarnessRepository::open(&path_str).expect("open file repo");
        repo.create(&harness("h1", 1, 0), &version("v1", "h1", 1, "orig"))
            .await
            .expect("seed harness");

        let (acquired_tx, acquired_rx) = std::sync::mpsc::channel();
        let holder_path = path_str.clone();
        let holder = std::thread::spawn(move || {
            let conn = rusqlite::Connection::open(&holder_path).expect("holder connection");
            conn.pragma_update(None, "busy_timeout", 5000)
                .expect("holder busy_timeout");
            // Acquire the write lock, signal, then hold it briefly before COMMIT.
            conn.execute_batch("BEGIN IMMEDIATE")
                .expect("holder acquires write lock");
            acquired_tx.send(()).expect("signal lock acquired");
            std::thread::sleep(Duration::from_millis(300));
            conn.execute_batch("COMMIT").expect("holder releases lock");
        });

        // Proceed only once the holder genuinely owns the write lock.
        acquired_rx.recv().expect("holder acquired the write lock");

        // This create contends for the write lock; busy_timeout must let it wait
        // out the ~300ms holder instead of returning a SQLITE_BUSY storage error.
        let res = repo
            .create(&harness("h2", 1, 0), &version("v-h2", "h2", 1, "after-wait"))
            .await;
        holder.join().expect("holder thread joins");
        res.expect("busy_timeout must absorb contention, not BUSY-fail");

        assert_eq!(
            repo.get_version("h2", 1)
                .await
                .expect("read back")
                .expect("h2's version persisted")
                .version_no,
            1
        );
        drop(repo);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
