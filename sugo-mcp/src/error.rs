//! Maps `sugo-core` domain errors into rmcp tool errors.
//!
//! Input validation happens at this boundary; the domain layer is trusted.
//! Each `CoreError` variant is mapped to a stable machine-readable code that
//! is carried in the structured `data` field of the rmcp [`ErrorData`] as
//! `{"code": "<code>"}`. The human-readable `message` is the domain error's
//! `Display` form and never depends on the code prefix, so callers branch on
//! the structured code rather than parsing the message string.

use rmcp::ErrorData;
use serde_json::json;
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
/// The human-readable `message` is the error's `Display` form; the stable
/// machine-readable code is carried in the structured `data` field as
/// `{"code": "<code>"}`.
pub fn to_tool_error(e: CoreError) -> ErrorData {
    let code = code_for(&e);
    ErrorData::internal_error(e.to_string(), Some(json!({ "code": code })))
}

/// Map a serde (de)serialization failure at the MCP boundary to a tool error.
///
/// Routes through [`to_tool_error`] via [`CoreError::Storage`] so that all
/// failure paths share the same structured `code` mapping.
pub fn serde_to_tool_error(e: serde_json::Error) -> ErrorData {
    to_tool_error(CoreError::Storage(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sugo_core::validate::{IssueCode, Severity, ValidationIssue};

    /// Extract the `code` string from a tool error's structured `data` field.
    fn code_of(e: &ErrorData) -> String {
        e.data
            .as_ref()
            .expect("data present")
            .get("code")
            .expect("code key")
            .as_str()
            .expect("code is string")
            .to_string()
    }

    #[test]
    fn maps_not_found_code() {
        let e = to_tool_error(CoreError::NotFound("h1".into()));
        assert_eq!(code_of(&e), "not_found");
        assert!(e.message.contains("h1"));
    }

    #[test]
    fn maps_lock_conflict_code() {
        let e = to_tool_error(CoreError::LockConflict { expected: 0, actual: 1 });
        assert_eq!(code_of(&e), "lock_conflict");
        assert!(e.message.contains("expected 0"));
        assert!(e.message.contains("actual 1"));
    }

    #[test]
    fn maps_validation_code() {
        let issues = vec![ValidationIssue {
            severity: Severity::Error,
            code: IssueCode::NoTerminal,
            message: "no terminal cell".into(),
            cell_id: None,
        }];
        let e = to_tool_error(CoreError::Validation(issues));
        assert_eq!(code_of(&e), "validation_failed");
    }

    #[test]
    fn maps_storage_code() {
        let e = to_tool_error(CoreError::Storage("disk full".into()));
        assert_eq!(code_of(&e), "storage_error");
        assert!(e.message.contains("disk full"));
    }

    #[test]
    fn message_has_no_code_prefix() {
        // The message must be the plain Display form, free of any `[code]` prefix.
        let e = to_tool_error(CoreError::NotFound("h1".into()));
        assert!(!e.message.starts_with('['));
        assert_eq!(e.message, "not found: h1");
    }

    #[test]
    fn serde_failure_maps_to_storage_code() {
        // A genuine serde error routed through the shared mapping yields the
        // storage_error code in the structured data field.
        let serde_err = serde_json::from_str::<serde_json::Value>("{invalid").unwrap_err();
        let e = serde_to_tool_error(serde_err);
        assert_eq!(code_of(&e), "storage_error");
    }
}
