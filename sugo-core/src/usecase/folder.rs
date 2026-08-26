//! フォルダの作成・改名・削除・ハーネス移動のユースケース。
//!
//! 名前の正規化（trim）・長さ検証・重複判定はここで行う。SQLite の UNIQUE は
//! `parent_id` が NULL のとき重複を弾けない（NULL 同士は別物と扱われる）ため、
//! 一意性は DB 制約ではなくこの層で担保する。

use crate::domain::folder::Folder;
use crate::error::CoreError;
use crate::ports::id_clock::IdClock;
use crate::ports::repository::HarnessRepository;
use crate::validate::{IssueCode, Severity, ValidationIssue};

/// フォルダ名の最大文字数（trim 後の char 数）。
pub const MAX_FOLDER_NAME_CHARS: usize = 64;

fn validation(message: &str) -> CoreError {
    CoreError::Validation(vec![ValidationIssue {
        severity: Severity::Error,
        code: IssueCode::InvalidFolderName,
        message: message.to_string(),
        cell_id: None,
    }])
}

/// 名前を trim し、長さを検証して返す。
fn normalize_name(name: &str) -> Result<String, CoreError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(validation("フォルダ名を入力してください"));
    }
    if trimmed.chars().count() > MAX_FOLDER_NAME_CHARS {
        return Err(validation("フォルダ名は64文字以内にしてください"));
    }
    Ok(trimmed.to_string())
}

/// フォルダを作成する。`sort_order` は既存フォルダ数を用いて作成順に採番する。
pub async fn create_folder(
    repo: &dyn HarnessRepository,
    clock: &dyn IdClock,
    name: &str,
) -> Result<Folder, CoreError> {
    let name = normalize_name(name)?;
    let existing = repo.list_folders().await?;
    if existing.iter().any(|(f, _)| f.name == name) {
        return Err(CoreError::Conflict(format!(
            "フォルダ「{name}」は既に存在します"
        )));
    }
    let now = clock.now_iso();
    let folder = Folder {
        id: clock.new_id(),
        name,
        parent_id: None,
        sort_order: existing.len() as i64,
        created_at: now.clone(),
        updated_at: now,
    };
    repo.create_folder(&folder).await?;
    Ok(folder)
}

/// フォルダを改名する。自分自身と同じ名前への改名は許可する。
pub async fn rename_folder(
    repo: &dyn HarnessRepository,
    clock: &dyn IdClock,
    folder_id: &str,
    name: &str,
) -> Result<(), CoreError> {
    let name = normalize_name(name)?;
    let existing = repo.list_folders().await?;
    if !existing.iter().any(|(f, _)| f.id == folder_id) {
        return Err(CoreError::NotFound(folder_id.to_string()));
    }
    if existing
        .iter()
        .any(|(f, _)| f.name == name && f.id != folder_id)
    {
        return Err(CoreError::Conflict(format!(
            "フォルダ「{name}」は既に存在します"
        )));
    }
    repo.rename_folder(folder_id, &name, &clock.now_iso()).await
}

/// フォルダを削除する。所属ハーネスは未分類に戻り、削除はされない。
/// 戻り値は `(削除したフォルダ名, 未分類に戻したハーネス件数)`。
pub async fn delete_folder(
    repo: &dyn HarnessRepository,
    folder_id: &str,
) -> Result<(String, i64), CoreError> {
    let existing = repo.list_folders().await?;
    let (folder, count) = existing
        .into_iter()
        .find(|(f, _)| f.id == folder_id)
        .ok_or_else(|| CoreError::NotFound(folder_id.to_string()))?;
    repo.delete_folder(folder_id).await?;
    Ok((folder.name, count))
}

/// ハーネスの所属フォルダを変更する。`folder_id` が `None` なら未分類へ。
pub async fn move_harness_to_folder(
    repo: &dyn HarnessRepository,
    harness_id: &str,
    folder_id: Option<&str>,
) -> Result<(), CoreError> {
    repo.move_harness_to_folder(harness_id, folder_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::repository::fake::{FakeIdClock, InMemoryHarnessRepository};

    #[tokio::test]
    async fn creates_folder_with_trimmed_name() {
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();
        let f = create_folder(&repo, &clock, "  開発  ").await.unwrap();
        assert_eq!(f.name, "開発");
        assert_eq!(f.sort_order, 0);
        assert!(f.parent_id.is_none());
    }

    #[tokio::test]
    async fn rejects_empty_name() {
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();
        let err = create_folder(&repo, &clock, "   ").await.unwrap_err();
        assert!(matches!(err, CoreError::Validation(_)));
    }

    #[tokio::test]
    async fn rejects_literal_empty_string_name() {
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();
        let err = create_folder(&repo, &clock, "").await.unwrap_err();
        assert!(matches!(err, CoreError::Validation(_)));
    }

    #[tokio::test]
    async fn folder_names_differing_only_in_case_are_not_duplicates() {
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();
        let a = create_folder(&repo, &clock, "Dev").await.unwrap();
        let b = create_folder(&repo, &clock, "dev").await.unwrap();
        assert_eq!(a.name, "Dev");
        assert_eq!(b.name, "dev");
        assert_ne!(a.id, b.id);
    }

    #[tokio::test]
    async fn creates_folder_with_single_character_name() {
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();
        let f = create_folder(&repo, &clock, "あ").await.unwrap();
        assert_eq!(f.name, "あ");
    }

    #[tokio::test]
    async fn rejects_name_longer_than_64_chars() {
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();
        let ok = "あ".repeat(64);
        create_folder(&repo, &clock, &ok).await.unwrap();
        let too_long = "い".repeat(65);
        let err = create_folder(&repo, &clock, &too_long).await.unwrap_err();
        assert!(matches!(err, CoreError::Validation(_)));
    }

    #[tokio::test]
    async fn rejects_duplicate_name() {
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();
        create_folder(&repo, &clock, "開発").await.unwrap();
        let err = create_folder(&repo, &clock, " 開発 ").await.unwrap_err();
        assert!(matches!(err, CoreError::Conflict(_)));
    }

    #[tokio::test]
    async fn sort_order_increments_by_creation() {
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();
        create_folder(&repo, &clock, "a").await.unwrap();
        let second = create_folder(&repo, &clock, "b").await.unwrap();
        assert_eq!(second.sort_order, 1);
    }

    #[tokio::test]
    async fn rename_to_same_name_as_itself_is_allowed() {
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();
        let f = create_folder(&repo, &clock, "開発").await.unwrap();
        rename_folder(&repo, &clock, &f.id, "開発").await.unwrap();
    }

    #[tokio::test]
    async fn rename_to_existing_other_name_conflicts() {
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();
        create_folder(&repo, &clock, "開発").await.unwrap();
        let b = create_folder(&repo, &clock, "調査").await.unwrap();
        let err = rename_folder(&repo, &clock, &b.id, "開発").await.unwrap_err();
        assert!(matches!(err, CoreError::Conflict(_)));
    }
}
