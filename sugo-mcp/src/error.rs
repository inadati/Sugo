//! Maps `sugo-core` domain errors into rmcp tool errors.
//!
//! Input validation happens at this boundary; the domain layer is trusted.
//! Each `CoreError` variant carries a stable machine-readable code embedded
//! in the message so that callers can branch on the failure kind.

use rmcp::ErrorData;
use sugo_core::error::CoreError;

/// Stable error code emitted for each [`CoreError`] variant.
fn code_for(e: &CoreError) -> &'static str {
    match e {
        CoreError::NotFound(_) => "not_found",
        CoreError::LockConflict { .. } => "lock_conflict",
        CoreError::Validation(_) => "validation_failed",
        CoreError::Storage(_) => "storage_error",
    }
}

/// Convert a domain error into an rmcp tool error response.
///
/// The resulting message is formatted as `[<code>] <human message>` so the
/// code can be parsed by the caller while remaining human readable.
pub fn to_tool_error(e: CoreError) -> ErrorData {
    let code = code_for(&e);
    ErrorData::internal_error(format!("[{code}] {e}"), None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sugo_core::validate::{Severity, ValidationIssue};

    #[test]
    fn maps_not_found_code() {
        let e = to_tool_error(CoreError::NotFound("h1".into()));
        assert!(e.message.contains("not_found"));
        assert!(e.message.contains("h1"));
    }

    #[test]
    fn maps_lock_conflict_code() {
        let e = to_tool_error(CoreError::LockConflict { expected: 0, actual: 1 });
        assert!(e.message.contains("lock_conflict"));
        assert!(e.message.contains("expected 0"));
        assert!(e.message.contains("actual 1"));
    }

    #[test]
    fn maps_validation_code() {
        let issues = vec![ValidationIssue {
            severity: Severity::Error,
            code: "no_terminal".into(),
            message: "no terminal cell".into(),
            cell_id: None,
        }];
        let e = to_tool_error(CoreError::Validation(issues));
        assert!(e.message.contains("validation_failed"));
    }

    #[test]
    fn maps_storage_code() {
        let e = to_tool_error(CoreError::Storage("disk full".into()));
        assert!(e.message.contains("storage_error"));
        assert!(e.message.contains("disk full"));
    }
}
