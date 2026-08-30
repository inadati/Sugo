//! ハーネスの改名ユースケース。
//!
//! 名前の正規化（trim）・長さ検証・重複時の自動採番はこの層で行う。
//! 重複はエラーにせず `名前 (2)` の形で採番して解決する。

use crate::error::CoreError;
use crate::ports::id_clock::IdClock;
use crate::ports::repository::HarnessRepository;
use crate::validate::{IssueCode, Severity, ValidationIssue};

/// ハーネス名の最大文字数（trim 後の char 数）。
pub const MAX_HARNESS_NAME_CHARS: usize = 64;

fn validation(message: &str) -> CoreError {
    CoreError::Validation(vec![ValidationIssue {
        severity: Severity::Error,
        code: IssueCode::InvalidHarnessName,
        message: message.to_string(),
        cell_id: None,
    }])
}

/// 名前を trim し、長さを検証して返す。`create_harness` からも使うため公開する。
pub fn normalize_name(name: &str) -> Result<String, CoreError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(validation("ハーネス名を入力してください"));
    }
    if trimmed.chars().count() > MAX_HARNESS_NAME_CHARS {
        return Err(validation("ハーネス名は64文字以内にしてください"));
    }
    Ok(trimmed.to_string())
}

/// `existing` に `desired` が無ければ `desired` をそのまま返す。
/// 衝突する場合は `desired (2)` から順に、`existing` に含まれない
/// 最小の番号を付けて返す。
///
/// 呼び出し側は改名対象のハーネス自身を `existing` から除外して渡すこと
/// （同名への改名を採番なしで素通りさせるため）。
pub fn resolve_unique_name(existing: &[String], desired: &str) -> String {
    if !existing.iter().any(|n| n == desired) {
        return desired.to_string();
    }
    let mut n = 2usize;
    loop {
        let candidate = format!("{desired} ({n})");
        if !existing.iter().any(|name| name == &candidate) {
            return candidate;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn returns_desired_when_no_conflict() {
        assert_eq!(resolve_unique_name(&names(&["alpha"]), "beta"), "beta");
    }

    #[test]
    fn appends_2_on_conflict() {
        assert_eq!(resolve_unique_name(&names(&["alpha"]), "alpha"), "alpha (2)");
    }

    #[test]
    fn skips_to_3_when_2_is_taken() {
        let existing = names(&["alpha", "alpha (2)"]);
        assert_eq!(resolve_unique_name(&existing, "alpha"), "alpha (3)");
    }

    #[test]
    fn fills_the_smallest_gap_in_the_sequence() {
        // (2) が空いていれば (4) が埋まっていても (2) を使う
        let existing = names(&["alpha", "alpha (3)", "alpha (4)"]);
        assert_eq!(resolve_unique_name(&existing, "alpha"), "alpha (2)");
    }

    #[test]
    fn already_numbered_name_is_kept_when_free() {
        // 「alpha (2)」への改名で existing に無ければそのまま通る
        assert_eq!(resolve_unique_name(&names(&["alpha"]), "alpha (2)"), "alpha (2)");
    }

    #[test]
    fn normalize_trims_surrounding_whitespace() {
        assert_eq!(normalize_name("  alpha  ").unwrap(), "alpha");
    }

    #[test]
    fn normalize_rejects_empty_name() {
        let err = normalize_name("   ").unwrap_err();
        assert!(matches!(err, CoreError::Validation(_)));
    }

    #[test]
    fn normalize_rejects_too_long_name() {
        let long = "あ".repeat(MAX_HARNESS_NAME_CHARS + 1);
        let err = normalize_name(&long).unwrap_err();
        assert!(matches!(err, CoreError::Validation(_)));
    }

    #[test]
    fn normalize_accepts_exactly_max_chars() {
        let exact = "あ".repeat(MAX_HARNESS_NAME_CHARS);
        assert_eq!(normalize_name(&exact).unwrap(), exact);
    }
}
