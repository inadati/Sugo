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
    /// A persistence-layer failure surfaced from an adapter.
    #[error("storage error: {0}")]
    Storage(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_conflict_message() {
        let e = CoreError::LockConflict { expected: 1, actual: 2 };
        assert_eq!(e.to_string(), "lock conflict: expected 1, actual 2");
    }
}
