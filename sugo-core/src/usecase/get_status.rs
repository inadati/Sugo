use crate::domain::cell::CellStatus;
use crate::error::CoreError;
use crate::ports::repository::HarnessRepository;

pub struct DraftDiffEntry {
    pub cell_id: String,
    pub name: String,
}

pub struct HarnessStatus {
    pub harness_id: String,
    pub name: String,
    pub current_version: i64,
    pub has_draft: bool,
    pub draft_diff: Vec<DraftDiffEntry>,
    pub definition_json: String,
}

pub async fn get_status(
    repo: &dyn HarnessRepository,
    harness_id: &str,
) -> Result<HarnessStatus, CoreError> {
    let (h, v) = repo
        .get(harness_id)
        .await?
        .ok_or_else(|| CoreError::NotFound(harness_id.to_string()))?;

    // draft_diff: 「active な前バージョンとの差分」。前バージョンに draft で存在
    // しなかったが現バージョンで draft になっているマスを列挙する。v1（前バージョン
    // 無し）なら現バージョンの draft セルをすべて追加扱いにする。
    let prev_drafts: std::collections::HashSet<String> = if v.version_no > 1 {
        match repo.get_version(harness_id, v.version_no - 1).await? {
            Some(prev) => prev
                .definition
                .cells
                .iter()
                .filter(|c| c.status == CellStatus::Draft)
                .map(|c| c.id.clone())
                .collect(),
            None => std::collections::HashSet::new(),
        }
    } else {
        std::collections::HashSet::new()
    };

    let draft_diff = v
        .definition
        .cells
        .iter()
        .filter(|c| c.status == CellStatus::Draft)
        .filter(|c| !prev_drafts.contains(&c.id))
        .map(|c| DraftDiffEntry { cell_id: c.id.clone(), name: c.name.clone() })
        .collect();
    let definition_json =
        serde_json::to_string(&v.definition).map_err(|e| CoreError::Storage(e.to_string()))?;
    Ok(HarnessStatus {
        harness_id: h.id,
        name: h.name,
        current_version: h.current_version,
        has_draft: h.has_draft,
        draft_diff,
        definition_json,
    })
}

pub struct HarnessSummary {
    pub harness_id: String,
    pub name: String,
    pub current_version: i64,
    pub has_draft: bool,
}

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
        // v1（前バージョン無し）では現バージョンの draft セルが全て追加扱い。
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
    async fn draft_diff_excludes_drafts_already_in_previous_version() {
        // c2 が v1 から既に draft の盤面を作り、edit_cell で v2 を生成する。
        // v2 の draft_diff は「前バージョンとの差分」のため、既存 draft の c2 を
        // 含まない（新規追加された draft マスだけを差分とする）。
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

        // v1 の draft_diff は c2 を追加扱い。
        let st1 = get_status(&repo, &out.harness_id).await.unwrap();
        assert_eq!(st1.draft_diff.len(), 1);
        assert_eq!(st1.draft_diff[0].cell_id, "c2");

        // edit_cell で v2 を生成（c2 は引き続き draft のまま）。
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

        // v2 では c2 が前バージョン(v1)でも既に draft だったので差分に出ない。
        let st2 = get_status(&repo, &out.harness_id).await.unwrap();
        assert_eq!(st2.current_version, 2);
        assert!(st2.has_draft);
        assert!(st2.draft_diff.is_empty());
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
