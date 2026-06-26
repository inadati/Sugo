use crate::validate::ValidationIssue;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("lock conflict: expected {expected}, actual {actual}")]
    LockConflict { expected: i64, actual: i64 },
    #[error("validation failed with {} issue(s)", .0.len())]
    Validation(Vec<ValidationIssue>),
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
