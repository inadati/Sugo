pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS harnesses (
  id              TEXT PRIMARY KEY,
  name            TEXT NOT NULL,
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
"#;
