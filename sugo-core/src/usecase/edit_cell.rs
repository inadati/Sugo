//! Use case for editing a single cell on a harness's current board.
//!
//! Applies an in-place cell change by appending a new immutable board version
//! (old versions stay untouched) under optimistic locking, keeping the harness
//! head pointed at the latest definition.

use crate::domain::cell::CellStatus;
use crate::domain::harness::BoardVersion;
use crate::error::CoreError;
use crate::ports::id_clock::IdClock;
use crate::ports::repository::HarnessRepository;
use crate::usecase::create_harness::content_hash;

/// Input for [`edit_cell`].
pub struct EditCellInput {
    /// Id of the harness to edit.
    pub harness_id: String,
    /// Id of the cell whose prompt is replaced.
    pub cell_id: String,
    /// New prompt text for the target cell.
    pub prompt: String,
    /// Caller's expected `lock_version`; mismatch yields a lock conflict.
    pub expected_lock_version: i64,
}

/// Output of [`edit_cell`].
#[derive(Debug)]
pub struct EditCellOutput {
    /// Id of the edited harness.
    pub harness_id: String,
    /// `version_no` of the newly appended board version.
    pub new_version: i64,
    /// New optimistic-lock version after the edit.
    pub lock_version: i64,
}

/// Edits a cell's prompt by appending a new immutable board version.
///
/// Board versions are immutable, so an edit never mutates the existing head:
/// the head definition is cloned, the target cell's prompt is replaced, and the
/// result is persisted as a fresh [`BoardVersion`] with `version_no + 1`. The
/// harness `lock_version` is bumped to advance the optimistic lock — the caller
/// must pass the current `lock_version` as `expected_lock_version`, and a
/// concurrent edit that already advanced the lock is rejected as a conflict
/// rather than silently overwriting another writer's version.
pub async fn edit_cell(
    repo: &dyn HarnessRepository,
    clock: &dyn IdClock,
    input: EditCellInput,
) -> Result<EditCellOutput, CoreError> {
    let (mut harness, head) = repo
        .get(&input.harness_id)
        .await?
        .ok_or_else(|| CoreError::NotFound(input.harness_id.clone()))?;

    if harness.lock_version != input.expected_lock_version {
        return Err(CoreError::LockConflict {
            expected: input.expected_lock_version,
            actual: harness.lock_version,
        });
    }

    let mut def = head.definition.clone();
    let cell = def
        .cells
        .iter_mut()
        .find(|c| c.id == input.cell_id)
        .ok_or_else(|| CoreError::NotFound(format!("cell {}", input.cell_id)))?;
    cell.prompt = input.prompt;

    let new_version_no = head.version_no + 1;
    let now = clock.now_iso();
    let new_version = BoardVersion {
        id: clock.new_id(),
        harness_id: harness.id.clone(),
        version_no: new_version_no,
        content_hash: content_hash(&def),
        definition: def.clone(),
        created_at: now.clone(),
    };

    let expected_lock = harness.lock_version;
    harness.current_version = new_version_no;
    harness.lock_version += 1;
    harness.has_draft = def.cells.iter().any(|c| c.status == CellStatus::Draft);
    harness.updated_at = now;

    repo.append_version(&harness, &new_version, expected_lock).await?;

    Ok(EditCellOutput {
        harness_id: harness.id,
        new_version: new_version_no,
        lock_version: harness.lock_version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::repository::fake::{FakeIdClock, InMemoryHarnessRepository};
    use crate::usecase::create_harness::{CreateHarnessInput, create_harness};

    async fn seed() -> (InMemoryHarnessRepository, FakeIdClock, String) {
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();
        let out = create_harness(
            &repo,
            &clock,
            CreateHarnessInput { name: "h".into(), definition: None },
        )
        .await
        .unwrap();
        (repo, clock, out.harness_id)
    }

    #[tokio::test]
    async fn edit_creates_new_version_and_bumps_lock() {
        let (repo, clock, id) = seed().await;
        let out = edit_cell(
            &repo,
            &clock,
            EditCellInput {
                harness_id: id.clone(),
                cell_id: "start".into(),
                prompt: "new".into(),
                expected_lock_version: 0,
            },
        )
        .await
        .unwrap();
        assert_eq!(out.new_version, 2);
        assert_eq!(out.lock_version, 1);
        // Old versions are immutable.
        let v1 = repo.get_version(&id, 1).await.unwrap().unwrap();
        assert_eq!(v1.definition.cells[0].prompt, "");
        let v2 = repo.get_version(&id, 2).await.unwrap().unwrap();
        assert_eq!(v2.definition.cells[0].prompt, "new");
    }

    #[tokio::test]
    async fn edit_missing_harness_is_not_found() {
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();
        let err = edit_cell(
            &repo,
            &clock,
            EditCellInput {
                harness_id: "nope".into(),
                cell_id: "start".into(),
                prompt: "x".into(),
                expected_lock_version: 0,
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, CoreError::NotFound(_)));
    }

    #[tokio::test]
    async fn edit_missing_cell_is_not_found() {
        let (repo, clock, id) = seed().await;
        let err = edit_cell(
            &repo,
            &clock,
            EditCellInput {
                harness_id: id,
                cell_id: "ghost".into(),
                prompt: "x".into(),
                expected_lock_version: 0,
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, CoreError::NotFound(_)));
    }

    #[tokio::test]
    async fn edit_with_stale_lock_conflicts() {
        let (repo, clock, id) = seed().await;
        let err = edit_cell(
            &repo,
            &clock,
            EditCellInput {
                harness_id: id,
                cell_id: "start".into(),
                prompt: "x".into(),
                expected_lock_version: 99,
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, CoreError::LockConflict { .. }));
    }
}
