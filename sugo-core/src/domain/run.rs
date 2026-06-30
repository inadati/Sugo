//! Run domain entity: an executing instance of a harness.

use serde::{Deserialize, Serialize};

/// Execution status of a run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Running,
    Done,
    Stalled,
    Disconnected,
    Closed,
}

impl std::fmt::Display for RunStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunStatus::Running => write!(f, "running"),
            RunStatus::Done => write!(f, "done"),
            RunStatus::Stalled => write!(f, "stalled"),
            RunStatus::Disconnected => write!(f, "disconnected"),
            RunStatus::Closed => write!(f, "closed"),
        }
    }
}

/// An executing instance of a harness, pinned to a specific board version.
///
/// A `Run` is created by `sugo_start` and advanced cell by cell via
/// `sugo_advance`. The board is pinned at creation so subsequent harness edits
/// cannot change the execution path.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub id: String,
    pub harness_id: String,
    /// The `version_no` of the board version this run was pinned to at creation.
    pub board_version_no: i64,
    /// The cell the run is currently positioned at.
    pub current_cell_id: String,
    pub status: RunStatus,
    /// Absolute path of the project directory; used to match jsonl `cwd` records.
    pub project_path: Option<String>,
    pub created_at: String,
    /// Last time a Nipper heartbeat was received for this run (RFC3339). None until first heartbeat.
    pub last_heartbeat_at: Option<String>,
    pub updated_at: String,
    /// Set to the timestamp when the last inject was sent; cleared by /inject-ack from Nipper.
    /// sugo_advance is blocked while this is Some (inject gate).
    pub inject_pending_since: Option<String>,
}
