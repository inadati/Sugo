//! Use case for batch-updating a harness in one new board version.

use crate::domain::cell::{Cell, CellStatus};
use crate::domain::edge::Edge;
use crate::domain::harness::BoardVersion;
use crate::error::CoreError;
use crate::ports::id_clock::IdClock;
use crate::ports::repository::HarnessRepository;
use crate::usecase::create_harness::content_hash;
use crate::validate::{IssueCode, Severity, ValidationIssue};

pub struct CellChange {
    pub cell_id: String,
    pub prompt: Option<String>,
    pub status: Option<CellStatus>,
    pub memo: Option<String>,
}

pub struct CellAdd {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub status: CellStatus,
    pub terminal: bool,
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
    pub cell_add: Vec<CellAdd>,
    pub edge_add: Vec<Edge>,
    pub edge_remove: Vec<EdgeKey>,
    pub cell_remove: Vec<String>,
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
        if let Some(ref m) = change.memo {
            cell.request_memo = m.trim().to_string();
        }
    }

    for add in &input.cell_add {
        if def.cells.iter().any(|c| c.id == add.id) {
            return Err(CoreError::Validation(vec![ValidationIssue {
                severity: Severity::Error,
                code: IssueCode::DuplicateCellId,
                message: format!("cell id '{}' already exists", add.id),
                cell_id: Some(add.id.clone()),
            }]));
        }
        def.cells.push(Cell {
            id: add.id.clone(),
            name: add.name.clone(),
            prompt: add.prompt.clone(),
            status: add.status,
            terminal: add.terminal,
            request_memo: "".into(),
        });
    }

    for cell_id in &input.cell_remove {
        if &def.start == cell_id {
            return Err(CoreError::Validation(vec![ValidationIssue {
                severity: Severity::Error,
                code: IssueCode::CannotRemoveStartCell,
                message: format!("cannot remove start cell '{}'", cell_id),
                cell_id: Some(cell_id.clone()),
            }]));
        }
    }
    def.cells.retain(|c| !input.cell_remove.contains(&c.id));
    def.edges
        .retain(|e| !input.cell_remove.contains(&e.from) && !input.cell_remove.contains(&e.to));

    for key in &input.edge_remove {
        def.edges
            .retain(|e| !(e.from == key.from && e.to == key.to && e.label == key.label));
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

    repo.append_version(&harness, &new_version, expected_lock)
        .await?;

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
                    request_memo: "".into(),
                },
                Cell {
                    id: "c2".into(),
                    name: "draft_cell".into(),
                    prompt: "".into(),
                    status: CellStatus::Draft,
                    terminal: true,
                    request_memo: "".into(),
                },
            ],
            edges: vec![Edge {
                from: "c1".into(),
                to: "c2".into(),
                label: "next".into(),
                guard: None,
            }],
        }
    }

    async fn seed() -> (InMemoryHarnessRepository, FakeIdClock, String) {
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();
        let out = create_harness(
            &repo,
            &clock,
            CreateHarnessInput {
                name: "h".into(),
                description: None,
                definition: Some(draft_board()),
            },
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
                    memo: None,
                }],
                cell_add: vec![],
                edge_add: vec![],
                edge_remove: vec![],
                cell_remove: vec![],
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
                    memo: None,
                }],
                cell_add: vec![],
                edge_add: vec![],
                edge_remove: vec![],
                cell_remove: vec![],
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
                cell_add: vec![],
                edge_add: vec![Edge {
                    from: "c2".into(),
                    to: "c1".into(),
                    label: "loop".into(),
                    guard: None,
                }],
                edge_remove: vec![],
                cell_remove: vec![],
            },
        )
        .await
        .unwrap();
        let (_, v) = repo.get(&id).await.unwrap().unwrap();
        assert!(
            v.definition
                .edges
                .iter()
                .any(|e| e.from == "c2" && e.label == "loop")
        );
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
                cell_add: vec![],
                edge_add: vec![],
                edge_remove: vec![EdgeKey {
                    from: "c1".into(),
                    to: "c2".into(),
                    label: "next".into(),
                }],
                cell_remove: vec![],
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
                cell_add: vec![],
                edge_add: vec![],
                edge_remove: vec![EdgeKey {
                    from: "ghost".into(),
                    to: "ghost2".into(),
                    label: "nope".into(),
                }],
                cell_remove: vec![],
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
                    memo: None,
                }],
                cell_add: vec![],
                edge_add: vec![Edge {
                    from: "c2".into(),
                    to: "c1".into(),
                    label: "retry".into(),
                    guard: None,
                }],
                edge_remove: vec![],
                cell_remove: vec![],
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
                    memo: None,
                }],
                cell_add: vec![],
                edge_add: vec![],
                edge_remove: vec![],
                cell_remove: vec![],
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
                cell_add: vec![],
                edge_add: vec![],
                edge_remove: vec![],
                cell_remove: vec![],
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, CoreError::LockConflict { .. }));
    }

    #[tokio::test]
    async fn update_clears_memo_and_activates_together() {
        let (repo, clock, id) = seed().await;
        // Draft へ降格済みの c2 に memo が入っている想定を作るため、まず memo を設定する。
        let (mut h, head) = repo.get(&id).await.unwrap().unwrap();
        let mut def = head.definition.clone();
        def.cells
            .iter_mut()
            .find(|c| c.id == "c2")
            .unwrap()
            .request_memo = "直して".into();
        let v2 = BoardVersion {
            id: "v2".into(),
            harness_id: id.clone(),
            version_no: h.current_version + 1,
            content_hash: crate::usecase::create_harness::content_hash(&def),
            definition: def,
            created_at: "now".into(),
        };
        h.current_version += 1;
        h.lock_version += 1;
        repo.append_version(&h, &v2, h.lock_version - 1)
            .await
            .unwrap();

        let out = update_harness(
            &repo,
            &clock,
            UpdateHarnessInput {
                harness_id: id.clone(),
                expected_lock_version: h.lock_version,
                cell_changes: vec![CellChange {
                    cell_id: "c2".into(),
                    prompt: Some("revised prompt".into()),
                    status: Some(CellStatus::Active),
                    memo: Some("".into()),
                }],
                cell_add: vec![],
                edge_add: vec![],
                edge_remove: vec![],
                cell_remove: vec![],
            },
        )
        .await
        .unwrap();

        assert_eq!(out.new_version, h.current_version + 1);
        let (hh, vv) = repo.get(&id).await.unwrap().unwrap();
        assert!(!hh.has_draft);
        let cell = vv.definition.cells.iter().find(|c| c.id == "c2").unwrap();
        assert_eq!(cell.prompt, "revised prompt");
        assert_eq!(cell.request_memo, "");
    }

    #[tokio::test]
    async fn update_trims_whitespace_only_memo_to_empty() {
        // The AI-facing memo-write path must normalize whitespace-only memo
        // the same way the GUI's set_cell_memo_inner does, so the same field
        // doesn't silently diverge in stored content depending on which
        // entry point wrote it.
        let (repo, clock, id) = seed().await;
        let out = update_harness(
            &repo,
            &clock,
            UpdateHarnessInput {
                harness_id: id.clone(),
                expected_lock_version: 0,
                cell_changes: vec![CellChange {
                    cell_id: "c1".into(),
                    prompt: None,
                    status: None,
                    memo: Some("   ".into()),
                }],
                cell_add: vec![],
                edge_add: vec![],
                edge_remove: vec![],
                cell_remove: vec![],
            },
        )
        .await
        .unwrap();

        assert_eq!(out.new_version, 2);
        let (_, v) = repo.get(&id).await.unwrap().unwrap();
        let cell = v.definition.cells.iter().find(|c| c.id == "c1").unwrap();
        assert_eq!(cell.request_memo, "");
        // update_harness never auto-couples status to memo (unlike the GUI's
        // set_cell_memo): status is driven solely by the change's own
        // `status` field, which was omitted here, so it stays Active.
        assert_eq!(cell.status, CellStatus::Active);
    }

    #[tokio::test]
    async fn update_adds_new_cell_as_active() {
        // seed()'s board already has a draft cell (c2), so activate it here
        // first to isolate has_draft's response to the newly added cell_add
        // cell alone (mirrors the pattern in
        // update_adds_new_cell_as_draft_sets_has_draft_true below).
        let (repo, clock, id) = seed().await;
        update_harness(
            &repo,
            &clock,
            UpdateHarnessInput {
                harness_id: id.clone(),
                expected_lock_version: 0,
                cell_changes: vec![CellChange {
                    cell_id: "c2".into(),
                    prompt: Some("done".into()),
                    status: Some(CellStatus::Active),
                    memo: None,
                }],
                cell_add: vec![CellAdd {
                    id: "c3".into(),
                    name: "third".into(),
                    prompt: "do third".into(),
                    status: CellStatus::Active,
                    terminal: true,
                }],
                edge_add: vec![],
                edge_remove: vec![],
                cell_remove: vec![],
            },
        )
        .await
        .unwrap();
        let (h, v) = repo.get(&id).await.unwrap().unwrap();
        assert!(!h.has_draft);
        let cell = v
            .definition
            .cells
            .iter()
            .find(|c| c.id == "c3")
            .expect("c3 added");
        assert_eq!(cell.name, "third");
        assert_eq!(cell.prompt, "do third");
        assert_eq!(cell.status, CellStatus::Active);
        assert!(cell.terminal);
    }

    #[tokio::test]
    async fn update_adds_new_cell_as_draft_sets_has_draft_true() {
        let (repo, clock, id) = seed().await;
        update_harness(
            &repo,
            &clock,
            UpdateHarnessInput {
                harness_id: id.clone(),
                expected_lock_version: 0,
                cell_changes: vec![CellChange {
                    cell_id: "c2".into(),
                    prompt: Some("done".into()),
                    status: Some(CellStatus::Active),
                    memo: None,
                }],
                cell_add: vec![CellAdd {
                    id: "c3".into(),
                    name: "third".into(),
                    prompt: "".into(),
                    status: CellStatus::Draft,
                    terminal: true,
                }],
                edge_add: vec![],
                edge_remove: vec![],
                cell_remove: vec![],
            },
        )
        .await
        .unwrap();
        let (h, _) = repo.get(&id).await.unwrap().unwrap();
        assert!(h.has_draft, "new draft cell must flip has_draft to true");
    }

    #[tokio::test]
    async fn update_adds_cell_and_connects_edge_in_same_call() {
        let (repo, clock, id) = seed().await;
        update_harness(
            &repo,
            &clock,
            UpdateHarnessInput {
                harness_id: id.clone(),
                expected_lock_version: 0,
                cell_changes: vec![],
                cell_add: vec![CellAdd {
                    id: "c3".into(),
                    name: "third".into(),
                    prompt: "do third".into(),
                    status: CellStatus::Active,
                    terminal: true,
                }],
                edge_add: vec![Edge {
                    from: "c1".into(),
                    to: "c3".into(),
                    label: "to_third".into(),
                    guard: None,
                }],
                edge_remove: vec![],
                cell_remove: vec![],
            },
        )
        .await
        .unwrap();
        let (_, v) = repo.get(&id).await.unwrap().unwrap();
        assert!(v.definition.cells.iter().any(|c| c.id == "c3"));
        assert!(
            v.definition
                .edges
                .iter()
                .any(|e| e.from == "c1" && e.to == "c3" && e.label == "to_third")
        );
    }

    #[tokio::test]
    async fn update_cell_add_duplicate_of_existing_id_is_validation_error() {
        let (repo, clock, id) = seed().await;
        let err = update_harness(
            &repo,
            &clock,
            UpdateHarnessInput {
                harness_id: id.clone(),
                expected_lock_version: 0,
                cell_changes: vec![],
                cell_add: vec![CellAdd {
                    id: "c1".into(),
                    name: "dup".into(),
                    prompt: "p".into(),
                    status: CellStatus::Active,
                    terminal: false,
                }],
                edge_add: vec![],
                edge_remove: vec![],
                cell_remove: vec![],
            },
        )
        .await
        .unwrap_err();
        match err {
            CoreError::Validation(issues) => {
                assert!(issues.iter().any(|i| i.code == IssueCode::DuplicateCellId));
            }
            other => panic!("expected Validation error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn update_cell_add_duplicate_within_batch_is_validation_error() {
        let (repo, clock, id) = seed().await;
        let err = update_harness(
            &repo,
            &clock,
            UpdateHarnessInput {
                harness_id: id.clone(),
                expected_lock_version: 0,
                cell_changes: vec![],
                cell_add: vec![
                    CellAdd {
                        id: "c3".into(),
                        name: "third".into(),
                        prompt: "p".into(),
                        status: CellStatus::Active,
                        terminal: false,
                    },
                    CellAdd {
                        id: "c3".into(),
                        name: "third-again".into(),
                        prompt: "p2".into(),
                        status: CellStatus::Active,
                        terminal: false,
                    },
                ],
                edge_add: vec![],
                edge_remove: vec![],
                cell_remove: vec![],
            },
        )
        .await
        .unwrap_err();
        match err {
            CoreError::Validation(issues) => {
                assert!(issues.iter().any(|i| i.code == IssueCode::DuplicateCellId));
            }
            other => panic!("expected Validation error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn update_removes_cell_and_cascades_connected_edges() {
        let (repo, clock, id) = seed().await;
        update_harness(
            &repo,
            &clock,
            UpdateHarnessInput {
                harness_id: id.clone(),
                expected_lock_version: 0,
                cell_changes: vec![],
                cell_add: vec![],
                edge_add: vec![],
                edge_remove: vec![],
                cell_remove: vec!["c2".into()],
            },
        )
        .await
        .unwrap();
        let (_, v) = repo.get(&id).await.unwrap().unwrap();
        assert!(!v.definition.cells.iter().any(|c| c.id == "c2"));
        assert!(v.definition.edges.is_empty(), "edge c1->c2 must be cascaded away");
    }

    #[tokio::test]
    async fn update_cannot_remove_start_cell_is_validation_error() {
        let (repo, clock, id) = seed().await;
        let err = update_harness(
            &repo,
            &clock,
            UpdateHarnessInput {
                harness_id: id.clone(),
                expected_lock_version: 0,
                cell_changes: vec![],
                cell_add: vec![],
                edge_add: vec![],
                edge_remove: vec![],
                cell_remove: vec!["c1".into()],
            },
        )
        .await
        .unwrap_err();
        match err {
            CoreError::Validation(issues) => {
                assert!(issues.iter().any(|i| i.code == IssueCode::CannotRemoveStartCell));
            }
            other => panic!("expected Validation error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn update_missing_cell_remove_id_is_silently_ignored() {
        let (repo, clock, id) = seed().await;
        let result = update_harness(
            &repo,
            &clock,
            UpdateHarnessInput {
                harness_id: id.clone(),
                expected_lock_version: 0,
                cell_changes: vec![],
                cell_add: vec![],
                edge_add: vec![],
                edge_remove: vec![],
                cell_remove: vec!["ghost".into()],
            },
        )
        .await;
        assert!(result.is_ok());
    }
}
