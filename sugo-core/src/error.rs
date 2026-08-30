//! The typed error returned by core usecases.

use crate::validate::ValidationIssue;

/// Errors a core usecase can return.
///
/// The domain layer raises these typed variants; the MCP boundary maps each to
/// a tool error carrying a stable code so agents can branch on failure kind.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// A referenced harness, board version, or cell was not found.
    #[error("not found: {0}")]
    NotFound(String),
    /// Optimistic-lock conflict: the caller's `expected` lock version did not
    /// match the stored `actual`, so the edit was rejected.
    #[error("lock conflict: expected {expected}, actual {actual}")]
    LockConflict {
        /// Lock version the caller expected.
        expected: i64,
        /// Lock version actually stored at head.
        actual: i64,
    },
    /// Structural validation failed; carries the offending issues.
    #[error("validation failed with {} issue(s)", .0.len())]
    Validation(Vec<ValidationIssue>),
    /// Harness has one or more draft cells; execution cannot start until all
    /// cells are active (SPEC 決定10c).
    #[error("harness has draft cells")]
    DraftCellsExist,
    /// `advance_run` was called on a run that is not in Running state.
    #[error("run is not running")]
    RunNotRunning,
    /// `delete_harness` was called while a Running run (heartbeated/updated
    /// within the active window) exists for the harness.
    #[error("harness has an active run")]
    ActiveRunExists,
    /// A persistence-layer failure surfaced from an adapter.
    #[error("storage error: {0}")]
    Storage(String),
    /// A uniqueness constraint enforced by a usecase was violated (e.g. a
    /// folder name that already exists). Distinct from `Validation`, which
    /// carries structural board issues.
    #[error("conflict: {0}")]
    Conflict(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_conflict_message() {
        let e = CoreError::LockConflict {
            expected: 1,
            actual: 2,
        };
        assert_eq!(e.to_string(), "lock conflict: expected 1, actual 2");
    }

    #[test]
    fn draft_cells_exist_message() {
        let e = CoreError::DraftCellsExist;
        assert_eq!(e.to_string(), "harness has draft cells");
    }

    #[test]
    fn run_not_running_message() {
        let e = CoreError::RunNotRunning;
        assert_eq!(e.to_string(), "run is not running");
    }

    #[test]
    fn conflict_message() {
        let e = CoreError::Conflict("folder name already exists: 開発".into());
        assert_eq!(e.to_string(), "conflict: folder name already exists: 開発");
    }
}
