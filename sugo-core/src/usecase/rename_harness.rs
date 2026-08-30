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

/// ハーネスを改名し、採番後の確定名を返す。
///
/// 重複判定は未削除の全ハーネス横断で行い（`repo.list()` はゴミ箱を除く）、
/// 改名対象自身は候補から除外する。名前が重複しても失敗せず、`名前 (2)` の
/// 形で採番する。ボードバージョンは変更しない。
pub async fn rename_harness(
    repo: &dyn HarnessRepository,
    clock: &dyn IdClock,
    harness_id: &str,
    desired_name: &str,
) -> Result<String, CoreError> {
    if repo.get(harness_id).await?.is_none() {
        return Err(CoreError::NotFound(harness_id.to_string()));
    }
    let desired = normalize_name(desired_name)?;
    let existing: Vec<String> = repo
        .list()
        .await?
        .into_iter()
        .filter(|h| h.id != harness_id)
        .map(|h| h.name)
        .collect();
    let final_name = resolve_unique_name(&existing, &desired);
    repo.rename_harness(harness_id, &final_name, &clock.now_iso())
        .await?;
    Ok(final_name)
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

    use crate::ports::repository::fake::{FakeIdClock, InMemoryHarnessRepository};
    use crate::usecase::create_harness::{create_harness, CreateHarnessInput};

    async fn seed(repo: &InMemoryHarnessRepository, clock: &FakeIdClock, name: &str) -> String {
        create_harness(
            repo,
            clock,
            CreateHarnessInput { name: name.into(), description: None, definition: None },
        )
        .await
        .unwrap()
        .harness_id
    }

    #[tokio::test]
    async fn renames_and_returns_final_name() {
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();
        let id = seed(&repo, &clock, "alpha").await;

        let final_name = rename_harness(&repo, &clock, &id, "beta").await.unwrap();

        assert_eq!(final_name, "beta");
        let (h, _) = repo.get(&id).await.unwrap().unwrap();
        assert_eq!(h.name, "beta");
    }

    #[tokio::test]
    async fn numbers_the_name_when_another_harness_has_it() {
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();
        seed(&repo, &clock, "alpha").await;
        let id = seed(&repo, &clock, "beta").await;

        let final_name = rename_harness(&repo, &clock, &id, "alpha").await.unwrap();

        assert_eq!(final_name, "alpha (2)");
    }

    #[tokio::test]
    async fn renaming_to_its_own_name_does_not_add_a_number() {
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();
        let id = seed(&repo, &clock, "alpha").await;

        let final_name = rename_harness(&repo, &clock, &id, "alpha").await.unwrap();

        assert_eq!(final_name, "alpha");
    }

    #[tokio::test]
    async fn trims_surrounding_whitespace_before_saving() {
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();
        let id = seed(&repo, &clock, "alpha").await;

        let final_name = rename_harness(&repo, &clock, &id, "  beta  ").await.unwrap();

        assert_eq!(final_name, "beta");
    }

    #[tokio::test]
    async fn empty_name_is_rejected() {
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();
        let id = seed(&repo, &clock, "alpha").await;

        let err = rename_harness(&repo, &clock, &id, "   ").await.unwrap_err();

        assert!(matches!(err, CoreError::Validation(_)));
        let (h, _) = repo.get(&id).await.unwrap().unwrap();
        assert_eq!(h.name, "alpha", "検証に失敗したら名前は変わらないこと");
    }

    #[tokio::test]
    async fn unknown_harness_is_not_found() {
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();

        let err = rename_harness(&repo, &clock, "nope", "beta").await.unwrap_err();

        assert!(matches!(err, CoreError::NotFound(_)));
    }

    #[tokio::test]
    async fn rename_does_not_bump_board_version() {
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();
        let id = seed(&repo, &clock, "alpha").await;

        rename_harness(&repo, &clock, &id, "beta").await.unwrap();

        let (h, v) = repo.get(&id).await.unwrap().unwrap();
        assert_eq!(h.current_version, 1);
        assert_eq!(h.lock_version, 0);
        assert_eq!(v.version_no, 1);
    }
}
