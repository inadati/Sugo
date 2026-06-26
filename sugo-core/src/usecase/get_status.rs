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
    let draft_diff = v
        .definition
        .cells
        .iter()
        .filter(|c| c.status == CellStatus::Draft)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::board::BoardDefinition;
    use crate::domain::cell::{Cell, CellStatus};
    use crate::ports::repository::fake::{FakeIdClock, InMemoryHarnessRepository};
    use crate::usecase::create_harness::{CreateHarnessInput, create_harness};

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
        assert_eq!(st.draft_diff.len(), 1);
        assert_eq!(st.draft_diff[0].cell_id, "c2");
    }
}
