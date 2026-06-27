//! Use case for reporting a harness's status, including its draft diff.
//!
//! Compares the current board version against the most recent draft-free
//! baseline to surface which draft cells were added, producing the
//! [`HarnessStatus`] (with [`DraftDiffEntry`] entries) consumed by callers.

use crate::domain::board::BoardDefinition;
use crate::domain::cell::CellStatus;
use crate::error::CoreError;
use crate::ports::repository::HarnessRepository;
use std::collections::HashSet;

/// A draft cell surfaced in [`HarnessStatus::draft_diff`].
///
/// Represents a cell that exists as a draft on the current board version but
/// was not present on the most recent draft-free (fully active) baseline.
pub struct DraftDiffEntry {
    /// Id of the draft cell.
    pub cell_id: String,
    /// Human-readable name of the draft cell.
    pub name: String,
}

/// Snapshot of a harness's current state plus its draft difference.
pub struct HarnessStatus {
    /// Id of the harness.
    pub harness_id: String,
    /// Name of the harness.
    pub name: String,
    /// `version_no` of the current head board version.
    pub current_version: i64,
    /// Whether the current board version contains any draft cell.
    pub has_draft: bool,
    /// Draft cells added relative to the last draft-free baseline version.
    pub draft_diff: Vec<DraftDiffEntry>,
    /// Current board definition (typed; callers project the fields they need).
    pub definition: BoardDefinition,
}

/// Returns the current status of a harness, including its `draft_diff`.
///
/// The `draft_diff` is the set of draft cells added relative to the last
/// *active* (draft-free) version, per the design's status semantics. Because
/// `board_versions` carries no active/draft marker and `edit_cell` produces a
/// new version even while drafts remain, the immediately preceding version is
/// not necessarily active: a draft introduced in v1 and left untouched through
/// an edit still lives in v2. Comparing v2 against v1 would wrongly drop such a
/// draft from the diff, contradicting `has_draft`.
///
/// To stay consistent with `has_draft`, the baseline is the most recent
/// version (scanning `version_no` downward from `current - 1` to 1) whose cells
/// are *all* active. Its cell ids form the baseline set; current draft cells
/// absent from that set are the diff. If no draft-free version exists, the
/// baseline is empty and every current draft cell is reported — so any draft
/// surviving across edits always appears in `draft_diff`.
pub async fn get_status(
    repo: &dyn HarnessRepository,
    harness_id: &str,
) -> Result<HarnessStatus, CoreError> {
    let (h, v) = repo
        .get(harness_id)
        .await?
        .ok_or_else(|| CoreError::NotFound(harness_id.to_string()))?;

    // Find the most recent draft-free (fully active) baseline version by
    // scanning version_no downward, and take its cell ids as the baseline set.
    // If none exists the baseline is empty, so every current draft is "added".
    let mut baseline_cells: HashSet<String> = HashSet::new();
    let mut vno = v.version_no - 1;
    while vno >= 1 {
        if let Some(prev) = repo.get_version(harness_id, vno).await? {
            let has_draft = prev
                .definition
                .cells
                .iter()
                .any(|c| c.status == CellStatus::Draft);
            if !has_draft {
                baseline_cells = prev.definition.cells.iter().map(|c| c.id.clone()).collect();
                break;
            }
        }
        vno -= 1;
    }

    // draft_diff: current draft cells whose ids are not present in the baseline
    // (active prev version) cell set.
    // Collect draft_diff first (it borrows v.definition.cells) so the typed
    // definition can then be moved into the status without a clone.
    let draft_diff = v
        .definition
        .cells
        .iter()
        .filter(|c| c.status == CellStatus::Draft)
        .filter(|c| !baseline_cells.contains(&c.id))
        .map(|c| DraftDiffEntry { cell_id: c.id.clone(), name: c.name.clone() })
        .collect();
    Ok(HarnessStatus {
        harness_id: h.id,
        name: h.name,
        current_version: h.current_version,
        has_draft: h.has_draft,
        draft_diff,
        definition: v.definition,
    })
}

/// Lightweight summary of a harness for listing purposes.
pub struct HarnessSummary {
    /// Id of the harness.
    pub harness_id: String,
    /// Name of the harness.
    pub name: String,
    /// `version_no` of the current head board version.
    pub current_version: i64,
    /// Whether the current board version contains any draft cell.
    pub has_draft: bool,
}

/// Returns a summary for every harness known to the repository.
pub async fn list_harness_summaries(
    repo: &dyn HarnessRepository,
) -> Result<Vec<HarnessSummary>, CoreError> {
    let harnesses = repo.list().await?;
    Ok(harnesses
        .into_iter()
        .map(|h| HarnessSummary {
            harness_id: h.id,
            name: h.name,
            current_version: h.current_version,
            has_draft: h.has_draft,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::board::BoardDefinition;
    use crate::domain::cell::{Cell, CellStatus};
    use crate::domain::edge::Edge;
    use crate::domain::harness::BoardVersion;
    use crate::ports::id_clock::IdClock;
    use crate::ports::repository::fake::{FakeIdClock, InMemoryHarnessRepository};
    use crate::usecase::create_harness::{CreateHarnessInput, create_harness};
    use crate::usecase::edit_cell::{EditCellInput, edit_cell};

    #[tokio::test]
    async fn status_lists_draft_cells() {
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();
        let def = BoardDefinition {
            schema_version: 1,
            start: "c1".into(),
            cells: vec![
                Cell {
                    id: "c1".into(),
                    name: "c1".into(),
                    prompt: "p".into(),
                    status: CellStatus::Active,
                    terminal: true,
                },
                Cell {
                    id: "c2".into(),
                    name: "draftcell".into(),
                    prompt: "".into(),
                    status: CellStatus::Draft,
                    terminal: false,
                },
            ],
            edges: vec![],
        };
        let out = create_harness(
            &repo,
            &clock,
            CreateHarnessInput { name: "h".into(), definition: Some(def) },
        )
        .await
        .unwrap();
        let st = get_status(&repo, &out.harness_id).await.unwrap();
        assert!(st.has_draft);
        // At v1 (no previous version) every draft cell on the current version counts as added.
        assert_eq!(st.draft_diff.len(), 1);
        assert_eq!(st.draft_diff[0].cell_id, "c2");
    }

    #[tokio::test]
    async fn get_status_missing_harness_is_not_found() {
        let repo = InMemoryHarnessRepository::new();
        let res = get_status(&repo, "nope").await;
        assert!(matches!(res, Err(CoreError::NotFound(_))));
    }

    #[tokio::test]
    async fn draft_diff_includes_drafts_surviving_across_edits() {
        // A board where c2 is a draft already in v1 (which is therefore never
        // active), then edit_cell produces v2 with c2 still a draft. Because no
        // draft-free baseline exists, c2 must remain listed in draft_diff so the
        // diff stays consistent with has_draft.
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();
        let def = BoardDefinition {
            schema_version: 1,
            start: "c1".into(),
            cells: vec![
                Cell {
                    id: "c1".into(),
                    name: "c1".into(),
                    prompt: "p".into(),
                    status: CellStatus::Active,
                    terminal: true,
                },
                Cell {
                    id: "c2".into(),
                    name: "draftcell".into(),
                    prompt: "".into(),
                    status: CellStatus::Draft,
                    terminal: false,
                },
            ],
            edges: vec![Edge {
                from: "c1".into(),
                to: "c2".into(),
                label: "l".into(),
                guard: None,
            }],
        };
        let out = create_harness(
            &repo,
            &clock,
            CreateHarnessInput { name: "h".into(), definition: Some(def) },
        )
        .await
        .unwrap();

        // v1: c2 is reported as an added draft (no baseline exists).
        let st1 = get_status(&repo, &out.harness_id).await.unwrap();
        assert_eq!(st1.draft_diff.len(), 1);
        assert_eq!(st1.draft_diff[0].cell_id, "c2");

        // edit_cell produces v2 (c2 stays a draft).
        edit_cell(
            &repo,
            &clock,
            EditCellInput {
                harness_id: out.harness_id.clone(),
                cell_id: "c2".into(),
                prompt: "filled".into(),
                expected_lock_version: 0,
            },
        )
        .await
        .unwrap();

        // v2: v1 was never active (it contained a draft), so there is no
        // draft-free baseline and c2 must still appear in draft_diff,
        // consistent with has_draft.
        let st2 = get_status(&repo, &out.harness_id).await.unwrap();
        assert_eq!(st2.current_version, 2);
        assert!(st2.has_draft);
        assert_eq!(st2.draft_diff.len(), 1);
        assert_eq!(st2.draft_diff[0].cell_id, "c2");
    }

    #[tokio::test]
    async fn draft_diff_only_lists_drafts_added_since_active_baseline() {
        // Start from a fully-active v1 (draft-free baseline). Editing a cell
        // produces an active v2 that is itself a baseline. Then introduce a new
        // draft cell (c3) in v3; only c3 should appear in draft_diff, measured
        // against the most recent active baseline (v2).
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();
        let def = BoardDefinition {
            schema_version: 1,
            start: "c1".into(),
            cells: vec![
                Cell {
                    id: "c1".into(),
                    name: "c1".into(),
                    prompt: "p".into(),
                    status: CellStatus::Active,
                    terminal: false,
                },
                Cell {
                    id: "c2".into(),
                    name: "c2".into(),
                    prompt: "p".into(),
                    status: CellStatus::Active,
                    terminal: true,
                },
            ],
            edges: vec![Edge {
                from: "c1".into(),
                to: "c2".into(),
                label: "l".into(),
                guard: None,
            }],
        };
        let out = create_harness(
            &repo,
            &clock,
            CreateHarnessInput { name: "h".into(), definition: Some(def) },
        )
        .await
        .unwrap();

        // v1 is draft-free: no draft_diff.
        let st1 = get_status(&repo, &out.harness_id).await.unwrap();
        assert!(!st1.has_draft);
        assert!(st1.draft_diff.is_empty());

        // edit_cell on c1 produces an active v2 (still draft-free).
        edit_cell(
            &repo,
            &clock,
            EditCellInput {
                harness_id: out.harness_id.clone(),
                cell_id: "c1".into(),
                prompt: "edited".into(),
                expected_lock_version: 0,
            },
        )
        .await
        .unwrap();

        // Manually append v3 that introduces a single new draft cell c3,
        // exercising the case where the active baseline is v2.
        let (mut h, head) = repo.get(&out.harness_id).await.unwrap().unwrap();
        let mut def3 = head.definition.clone();
        def3.cells.push(Cell {
            id: "c3".into(),
            name: "newdraft".into(),
            prompt: "".into(),
            status: CellStatus::Draft,
            terminal: false,
        });
        let expected_lock = h.lock_version;
        let new_version = BoardVersion {
            id: clock.new_id(),
            harness_id: h.id.clone(),
            version_no: head.version_no + 1,
            content_hash: "hash".into(),
            definition: def3,
            created_at: clock.now_iso(),
        };
        h.current_version = new_version.version_no;
        h.lock_version += 1;
        h.has_draft = true;
        repo.append_version(&h, &new_version, expected_lock)
            .await
            .unwrap();

        // v3: baseline is the most recent active version (v2). Only c3 is added.
        let st3 = get_status(&repo, &out.harness_id).await.unwrap();
        assert_eq!(st3.current_version, 3);
        assert!(st3.has_draft);
        assert_eq!(st3.draft_diff.len(), 1);
        assert_eq!(st3.draft_diff[0].cell_id, "c3");
    }

    #[tokio::test]
    async fn list_harness_summaries_returns_all() {
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();
        for n in ["a", "b"] {
            create_harness(
                &repo,
                &clock,
                CreateHarnessInput { name: n.into(), definition: None },
            )
            .await
            .unwrap();
        }
        let mut summaries = list_harness_summaries(&repo).await.unwrap();
        summaries.sort_by(|x, y| x.name.cmp(&y.name));
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].name, "a");
        assert_eq!(summaries[1].name, "b");
        assert_eq!(summaries[0].current_version, 1);
        assert!(!summaries[0].has_draft);
    }
}
