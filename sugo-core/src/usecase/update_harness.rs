//! Use case for batch-updating a harness in one new board version.

use crate::domain::cell::CellStatus;
use crate::domain::edge::Edge;
use crate::domain::harness::BoardVersion;
use crate::error::CoreError;
use crate::ports::id_clock::IdClock;
use crate::ports::repository::HarnessRepository;
use crate::usecase::create_harness::content_hash;

pub struct CellChange {
    pub cell_id: String,
    pub prompt: Option<String>,
    pub status: Option<CellStatus>,
}

pub struct EdgeKey {
    pub from: String,
    pub to: String,
    pub label: String,
}

pub struct UpdateHarnessInput {
    pub harness_id: String,
    pub expected_lock_version: i64,
    pub cell_changes: Vec<CellChange>,
    pub edge_add: Vec<Edge>,
    pub edge_remove: Vec<EdgeKey>,
}

#[derive(Debug)]
pub struct UpdateHarnessOutput {
    pub harness_id: String,
    pub new_version: i64,
    pub lock_version: i64,
}

pub async fn update_harness(
    repo: &dyn HarnessRepository,
    clock: &dyn IdClock,
    input: UpdateHarnessInput,
) -> Result<UpdateHarnessOutput, CoreError> {
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

    for change in &input.cell_changes {
        let cell = def
            .cells
            .iter_mut()
            .find(|c| c.id == change.cell_id)
            .ok_or_else(|| CoreError::NotFound(format!("cell {}", change.cell_id)))?;
        if let Some(ref p) = change.prompt {
            cell.prompt = p.clone();
        }
        if let Some(s) = change.status {
            cell.status = s;
        }
    }

    for key in &input.edge_remove {
        def.edges.retain(|e| !(e.from == key.from && e.to == key.to && e.label == key.label));
    }

    def.edges.extend(input.edge_add.into_iter());

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

    Ok(UpdateHarnessOutput {
        harness_id: harness.id,
        new_version: new_version_no,
        lock_version: harness.lock_version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::board::BoardDefinition;
    use crate::domain::cell::{Cell, CellStatus};
    use crate::domain::edge::Edge;
    use crate::ports::repository::fake::{FakeIdClock, InMemoryHarnessRepository};
    use crate::usecase::create_harness::{CreateHarnessInput, create_harness};

    fn draft_board() -> BoardDefinition {
        BoardDefinition {
            schema_version: 1,
            start: "c1".into(),
            cells: vec![
                Cell {
                    id: "c1".into(),
                    name: "active".into(),
                    prompt: "do active".into(),
                    status: CellStatus::Active,
                    terminal: false,
                },
                Cell {
                    id: "c2".into(),
                    name: "draft_cell".into(),
                    prompt: "".into(),
                    status: CellStatus::Draft,
                    terminal: true,
                },
            ],
            edges: vec![
                Edge { from: "c1".into(), to: "c2".into(), label: "next".into(), guard: None },
            ],
        }
    }

    async fn seed() -> (InMemoryHarnessRepository, FakeIdClock, String) {
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();
        let out = create_harness(
            &repo,
            &clock,
            CreateHarnessInput { name: "h".into(), description: None, definition: Some(draft_board()) },
        )
        .await
        .unwrap();
        (repo, clock, out.harness_id)
    }

    #[tokio::test]
    async fn update_activates_draft_cell_and_clears_has_draft() {
        let (repo, clock, id) = seed().await;
        let out = update_harness(
            &repo,
            &clock,
            UpdateHarnessInput {
                harness_id: id.clone(),
                expected_lock_version: 0,
                cell_changes: vec![CellChange {
                    cell_id: "c2".into(),
                    prompt: Some("done".into()),
                    status: Some(CellStatus::Active),
                }],
                edge_add: vec![],
                edge_remove: vec![],
            },
        )
        .await
        .unwrap();
        assert_eq!(out.new_version, 2);
        assert_eq!(out.lock_version, 1);
        let (h, v) = repo.get(&id).await.unwrap().unwrap();
        assert!(!h.has_draft);
        let cell = v.definition.cells.iter().find(|c| c.id == "c2").unwrap();
        assert_eq!(cell.status, CellStatus::Active);
        assert_eq!(cell.prompt, "done");
    }

    #[tokio::test]
    async fn update_changes_prompts_without_status_change() {
        let (repo, clock, id) = seed().await;
        update_harness(
            &repo,
            &clock,
            UpdateHarnessInput {
                harness_id: id.clone(),
                expected_lock_version: 0,
                cell_changes: vec![CellChange {
                    cell_id: "c1".into(),
                    prompt: Some("updated prompt".into()),
                    status: None,
                }],
                edge_add: vec![],
                edge_remove: vec![],
            },
        )
        .await
        .unwrap();
        let (_, v) = repo.get(&id).await.unwrap().unwrap();
        let cell = v.definition.cells.iter().find(|c| c.id == "c1").unwrap();
        assert_eq!(cell.prompt, "updated prompt");
        assert_eq!(cell.status, CellStatus::Active);
    }

    #[tokio::test]
    async fn update_adds_edge() {
        let (repo, clock, id) = seed().await;
        update_harness(
            &repo,
            &clock,
            UpdateHarnessInput {
                harness_id: id.clone(),
                expected_lock_version: 0,
                cell_changes: vec![],
                edge_add: vec![Edge {
                    from: "c2".into(),
                    to: "c1".into(),
                    label: "loop".into(),
                    guard: None,
                }],
                edge_remove: vec![],
            },
        )
        .await
        .unwrap();
        let (_, v) = repo.get(&id).await.unwrap().unwrap();
        assert!(v.definition.edges.iter().any(|e| e.from == "c2" && e.label == "loop"));
    }

    #[tokio::test]
    async fn update_removes_edge() {
        let (repo, clock, id) = seed().await;
        update_harness(
            &repo,
            &clock,
            UpdateHarnessInput {
                harness_id: id.clone(),
                expected_lock_version: 0,
                cell_changes: vec![],
                edge_add: vec![],
                edge_remove: vec![EdgeKey {
                    from: "c1".into(),
                    to: "c2".into(),
                    label: "next".into(),
                }],
            },
        )
        .await
        .unwrap();
        let (_, v) = repo.get(&id).await.unwrap().unwrap();
        assert!(v.definition.edges.is_empty());
    }

    #[tokio::test]
    async fn update_missing_edge_remove_is_silently_ignored() {
        let (repo, clock, id) = seed().await;
        let result = update_harness(
            &repo,
            &clock,
            UpdateHarnessInput {
                harness_id: id.clone(),
                expected_lock_version: 0,
                cell_changes: vec![],
                edge_add: vec![],
                edge_remove: vec![EdgeKey {
                    from: "ghost".into(),
                    to: "ghost2".into(),
                    label: "nope".into(),
                }],
            },
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn update_applies_cell_and_edge_changes_together() {
        let (repo, clock, id) = seed().await;
        update_harness(
            &repo,
            &clock,
            UpdateHarnessInput {
                harness_id: id.clone(),
                expected_lock_version: 0,
                cell_changes: vec![CellChange {
                    cell_id: "c2".into(),
                    prompt: Some("filled".into()),
                    status: Some(CellStatus::Active),
                }],
                edge_add: vec![Edge {
                    from: "c2".into(),
                    to: "c1".into(),
                    label: "retry".into(),
                    guard: None,
                }],
                edge_remove: vec![],
            },
        )
        .await
        .unwrap();
        let (h, v) = repo.get(&id).await.unwrap().unwrap();
        assert!(!h.has_draft);
        assert!(v.definition.edges.iter().any(|e| e.label == "retry"));
    }

    #[tokio::test]
    async fn update_missing_cell_id_is_not_found() {
        let (repo, clock, id) = seed().await;
        let err = update_harness(
            &repo,
            &clock,
            UpdateHarnessInput {
                harness_id: id.clone(),
                expected_lock_version: 0,
                cell_changes: vec![CellChange {
                    cell_id: "ghost".into(),
                    prompt: None,
                    status: Some(CellStatus::Active),
                }],
                edge_add: vec![],
                edge_remove: vec![],
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, CoreError::NotFound(_)));
    }

    #[tokio::test]
    async fn update_stale_lock_version_is_conflict() {
        let (repo, clock, id) = seed().await;
        let err = update_harness(
            &repo,
            &clock,
            UpdateHarnessInput {
                harness_id: id.clone(),
                expected_lock_version: 99,
                cell_changes: vec![],
                edge_add: vec![],
                edge_remove: vec![],
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, CoreError::LockConflict { .. }));
    }
}
