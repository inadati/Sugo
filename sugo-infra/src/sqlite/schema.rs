//! SQLite schema (DDL) for the P1 harness store.
//!
//! Holds the single [`SCHEMA`] constant applied once at repository
//! initialization. Kept separate from the repository logic so the table shapes
//! and constraints can be read in one place.

/// DDL applied at repository initialization.
///
/// Creates the P1 tables (`harnesses`, `board_versions`), the P2 `runs`
/// table, and the `cell_positions` table.  The `board_versions.harness_id`
/// foreign key is enforced only when the connection has
/// `PRAGMA foreign_keys = ON`, which the repository sets on open; see
/// [`crate::sqlite::SqliteHarnessRepository`]. The
/// `UNIQUE(harness_id, version_no)` constraint guarantees board-version
/// immutability by rejecting silent overwrites of an existing version number.
/// `cell_positions` holds display-only coordinates, not board semantics, so
/// it is kept out of `board_versions.definition_json`.
pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS folders (
  id         TEXT PRIMARY KEY,
  name       TEXT NOT NULL,
  parent_id  TEXT REFERENCES folders(id),
  sort_order INTEGER NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS harnesses (
  id              TEXT PRIMARY KEY,
  name            TEXT NOT NULL,
  description     TEXT,
  folder_id       TEXT REFERENCES folders(id),
  current_version INTEGER NOT NULL,
  has_draft       INTEGER NOT NULL,
  lock_version    INTEGER NOT NULL,
  created_at      TEXT NOT NULL,
  updated_at      TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS board_versions (
  id              TEXT PRIMARY KEY,
  harness_id      TEXT NOT NULL REFERENCES harnesses(id),
  version_no      INTEGER NOT NULL,
  definition_json TEXT NOT NULL,
  content_hash    TEXT NOT NULL,
  created_at      TEXT NOT NULL,
  UNIQUE(harness_id, version_no)
);
CREATE TABLE IF NOT EXISTS runs (
  id               TEXT PRIMARY KEY,
  harness_id       TEXT NOT NULL,
  board_version_no INTEGER NOT NULL,
  current_cell_id  TEXT NOT NULL,
  status           TEXT NOT NULL DEFAULT 'running',
  project_path     TEXT,
  created_at       TEXT NOT NULL,
  updated_at       TEXT NOT NULL,
  last_heartbeat_at TEXT,
  inject_pending_since TEXT,
  current_step_token TEXT
);
CREATE TABLE IF NOT EXISTS cell_positions (
  harness_id TEXT NOT NULL REFERENCES harnesses(id),
  cell_id    TEXT NOT NULL,
  x          REAL NOT NULL,
  y          REAL NOT NULL,
  PRIMARY KEY (harness_id, cell_id)
);
"#;
