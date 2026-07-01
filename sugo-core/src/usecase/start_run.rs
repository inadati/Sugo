//! Use case for starting a harness run.

use crate::domain::cell::CellStatus;
use crate::domain::run::{Run, RunStatus};
use crate::error::CoreError;
use crate::ports::id_clock::IdClock;
use crate::ports::repository::HarnessRepository;
use crate::ports::run_repository::RunRepository;

/// One outgoing edge shown to the agent when a run lands on a cell.
#[derive(Debug)]
pub struct EdgeInfo {
    pub label: String,
    pub to_cell_id: String,
    pub to_cell_name: String,
    pub guard: Option<String>,
}

#[derive(Debug)]
pub struct StartRunInput {
    pub harness_id: String,
    /// Absolute project path for jsonl stall detection. None when not needed.
    pub project_path: Option<String>,
}

#[derive(Debug)]
pub struct StartRunOutput {
    pub run_id: String,
    pub cell_name: String,
    pub prompt: String,
    pub edges: Vec<EdgeInfo>,
}

/// Create a new run for `harness_id` and return the start cell's prompt + edges.
///
/// Fails with [`CoreError::DraftCellsExist`] if the harness has any draft cell
/// (SPEC 決定10c), and with [`CoreError::NotFound`] if the harness does not exist.
pub async fn start_run(
    repo: &dyn HarnessRepository,
    run_repo: &dyn RunRepository,
    clock: &dyn IdClock,
    input: StartRunInput,
) -> Result<StartRunOutput, CoreError> {
    let (harness, version) = repo
        .get(&input.harness_id)
        .await?
        .ok_or_else(|| CoreError::NotFound(input.harness_id.clone()))?;

    // SPEC 決定10c: draft cell が 1 つでもあれば実行開始をハードエラーにする
    if version.definition.cells.iter().any(|c| c.status == CellStatus::Draft) {
        return Err(CoreError::DraftCellsExist);
    }

    let def = &version.definition;
    let start_cell = def
        .cells
        .iter()
        .find(|c| c.id == def.start)
        .ok_or_else(|| CoreError::Storage("start cell not found in board".into()))?;

    let now = clock.now_iso();
    let run = Run {
        id: clock.new_id(),
        harness_id: harness.id,
        board_version_no: version.version_no,
        current_cell_id: start_cell.id.clone(),
        status: RunStatus::Running,
        project_path: input.project_path,
        created_at: now.clone(),
        last_heartbeat_at: None,
        updated_at: now,
        inject_pending_since: None,
    };
    let run_id = run.id.clone();
    run_repo.create(&run).await?;

    let edges = def
        .edges
        .iter()
        .filter(|e| e.from == start_cell.id)
        .map(|e| {
            let to_name = def
                .cells
                .iter()
                .find(|c| c.id == e.to)
                .map(|c| c.name.clone())
                .unwrap_or_default();
            EdgeInfo {
                label: e.label.clone(),
                to_cell_id: e.to.clone(),
                to_cell_name: to_name,
                guard: e.guard.as_ref().map(|g| g.expr.clone()),
            }
        })
        .collect();

    Ok(StartRunOutput {
        run_id,
        cell_name: start_cell.name.clone(),
        prompt: start_cell.prompt.clone(),
        edges,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::board::BoardDefinition;
    use crate::domain::cell::{Cell, CellStatus};
    use crate::domain::edge::Edge;
    use crate::domain::run::RunStatus;
    use crate::error::CoreError;
    use crate::ports::repository::fake::{FakeIdClock, InMemoryHarnessRepository};
    use crate::ports::run_repository::fake::InMemoryRunRepository;
    use crate::usecase::create_harness::{CreateHarnessInput, create_harness};

    fn two_cell_board() -> BoardDefinition {
        BoardDefinition {
            schema_version: 1,
            start: "c1".into(),
            cells: vec![
                Cell {
                    id: "c1".into(),
                    name: "intro".into(),
                    prompt: "do intro".into(),
                    status: CellStatus::Active,
                    terminal: false,
                    request_memo: "".into(),
                },
                Cell {
                    id: "c2".into(),
                    name: "done".into(),
                    prompt: "wrap up".into(),
                    status: CellStatus::Active,
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

    #[tokio::test]
    async fn start_run_creates_run_at_start_cell() {
        let repo = InMemoryHarnessRepository::new();
        let run_repo = InMemoryRunRepository::new();
        let clock = FakeIdClock::new();
        let out = create_harness(
            &repo,
            &clock,
            CreateHarnessInput { name: "h".into(), description: None, definition: Some(two_cell_board()) },
        )
        .await
        .unwrap();
        let result = start_run(
            &repo,
            &run_repo,
            &clock,
            StartRunInput { harness_id: out.harness_id.clone(), project_path: None },
        )
        .await
        .unwrap();
        assert_eq!(result.cell_name, "intro");
        assert_eq!(result.prompt, "do intro");
        assert_eq!(result.edges.len(), 1);
        assert_eq!(result.edges[0].label, "next");
        let run = run_repo.get(&result.run_id).await.unwrap().unwrap();
        assert_eq!(run.current_cell_id, "c1");
        assert_eq!(run.board_version_no, 1);
        assert_eq!(run.status, RunStatus::Running);
    }

    #[tokio::test]
    async fn start_run_rejects_draft_cells() {
        let repo = InMemoryHarnessRepository::new();
        let run_repo = InMemoryRunRepository::new();
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
                    request_memo: "".into(),
                },
                Cell {
                    id: "c2".into(),
                    name: "draft".into(),
                    prompt: "".into(),
                    status: CellStatus::Draft,
                    terminal: false,
                    request_memo: "".into(),
                },
            ],
            edges: vec![],
        };
        let out = create_harness(
            &repo,
            &clock,
            CreateHarnessInput { name: "h".into(), description: None, definition: Some(def) },
        )
        .await
        .unwrap();
        let err = start_run(
            &repo,
            &run_repo,
            &clock,
            StartRunInput { harness_id: out.harness_id, project_path: None },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, CoreError::DraftCellsExist));
    }

    #[tokio::test]
    async fn start_run_missing_harness_is_not_found() {
        let repo = InMemoryHarnessRepository::new();
        let run_repo = InMemoryRunRepository::new();
        let clock = FakeIdClock::new();
        let err = start_run(
            &repo,
            &run_repo,
            &clock,
            StartRunInput { harness_id: "nope".into(), project_path: None },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, CoreError::NotFound(_)));
    }
}
