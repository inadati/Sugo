//! Use case for advancing a run from one cell to the next.

use crate::domain::run::RunStatus;
use crate::error::CoreError;
use crate::ports::id_clock::IdClock;
use crate::ports::repository::HarnessRepository;
use crate::ports::run_repository::RunRepository;
use crate::usecase::start_run::EdgeInfo;

#[derive(Debug)]
pub struct AdvanceRunInput {
    pub run_id: String,
    /// Edge label to follow from the current cell.
    pub edge_label: String,
}

#[derive(Debug)]
pub struct AdvanceRunOutput {
    pub cell_name: String,
    pub prompt: String,
    pub terminal: bool,
    pub edges: Vec<EdgeInfo>,
}

/// Advance a running run along `edge_label` to the next cell.
///
/// Returns the next cell's prompt and its outgoing edges.
/// Fails with [`CoreError::NotFound`] when the run does not exist, is not
/// `Running`, or `edge_label` has no matching outgoing edge from the current cell.
pub async fn advance_run(
    repo: &dyn HarnessRepository,
    run_repo: &dyn RunRepository,
    clock: &dyn IdClock,
    input: AdvanceRunInput,
) -> Result<AdvanceRunOutput, CoreError> {
    let mut run = run_repo
        .get(&input.run_id)
        .await?
        .ok_or_else(|| CoreError::NotFound(format!("run not found: {}", input.run_id)))?;

    if run.status != RunStatus::Running {
        return Err(CoreError::RunNotRunning);
    }

    // Fetch the pinned board version (not the current head, which may have changed)
    let version = repo
        .get_version(&run.harness_id, run.board_version_no)
        .await?
        .ok_or_else(|| CoreError::Storage(format!("pinned version {} missing", run.board_version_no)))?;

    let def = &version.definition;

    // Find the matching outgoing edge from current cell
    let edge = def
        .edges
        .iter()
        .find(|e| e.from == run.current_cell_id && e.label == input.edge_label)
        .ok_or_else(|| {
            CoreError::NotFound(format!(
                "edge '{}' not found from cell '{}'",
                input.edge_label, run.current_cell_id
            ))
        })?;

    let next_cell = def
        .cells
        .iter()
        .find(|c| c.id == edge.to)
        .ok_or_else(|| CoreError::Storage(format!("target cell '{}' not found", edge.to)))?;

    // Transition the run
    run.current_cell_id = next_cell.id.clone();
    if next_cell.terminal {
        run.status = RunStatus::Done;
    }
    run.updated_at = clock.now_iso();
    run_repo.update(&run).await?;

    let outgoing_edges = def
        .edges
        .iter()
        .filter(|e| e.from == next_cell.id)
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

    Ok(AdvanceRunOutput {
        cell_name: next_cell.name.clone(),
        prompt: next_cell.prompt.clone(),
        terminal: next_cell.terminal,
        edges: outgoing_edges,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::board::BoardDefinition;
    use crate::domain::cell::{Cell, CellStatus};
    use crate::domain::edge::{Edge, Guard};
    use crate::domain::run::RunStatus;
    use crate::error::CoreError;
    use crate::ports::repository::fake::{FakeIdClock, InMemoryHarnessRepository};
    use crate::ports::run_repository::fake::InMemoryRunRepository;
    use crate::usecase::create_harness::{CreateHarnessInput, create_harness};
    use crate::usecase::start_run::{StartRunInput, start_run};

    fn linear_board() -> BoardDefinition {
        BoardDefinition {
            schema_version: 1,
            start: "c1".into(),
            cells: vec![
                Cell {
                    id: "c1".into(),
                    name: "step1".into(),
                    prompt: "do step1".into(),
                    status: CellStatus::Active,
                    terminal: false,
                },
                Cell {
                    id: "c2".into(),
                    name: "step2".into(),
                    prompt: "do step2".into(),
                    status: CellStatus::Active,
                    terminal: false,
                },
                Cell {
                    id: "c3".into(),
                    name: "end".into(),
                    prompt: "done".into(),
                    status: CellStatus::Active,
                    terminal: true,
                },
            ],
            edges: vec![
                Edge { from: "c1".into(), to: "c2".into(), label: "next".into(), guard: None },
                Edge { from: "c2".into(), to: "c3".into(), label: "finish".into(), guard: None },
            ],
        }
    }

    async fn setup() -> (InMemoryHarnessRepository, InMemoryRunRepository, FakeIdClock, String) {
        let repo = InMemoryHarnessRepository::new();
        let run_repo = InMemoryRunRepository::new();
        let clock = FakeIdClock::new();
        let h = create_harness(
            &repo,
            &clock,
            CreateHarnessInput { name: "h".into(), description: None, definition: Some(linear_board()) },
        )
        .await
        .unwrap();
        let s = start_run(
            &repo,
            &run_repo,
            &clock,
            StartRunInput { harness_id: h.harness_id.clone(), project_path: None },
        )
        .await
        .unwrap();
        (repo, run_repo, clock, s.run_id)
    }

    #[tokio::test]
    async fn advance_moves_to_next_cell() {
        let (repo, run_repo, clock, run_id) = setup().await;
        let out = advance_run(
            &repo,
            &run_repo,
            &clock,
            AdvanceRunInput { run_id: run_id.clone(), edge_label: "next".into() },
        )
        .await
        .unwrap();
        assert_eq!(out.cell_name, "step2");
        assert_eq!(out.prompt, "do step2");
        assert!(!out.terminal);
        assert_eq!(out.edges.len(), 1);
        assert_eq!(out.edges[0].label, "finish");
        let run = run_repo.get(&run_id).await.unwrap().unwrap();
        assert_eq!(run.current_cell_id, "c2");
        assert_eq!(run.status, RunStatus::Running);
    }

    #[tokio::test]
    async fn advance_to_terminal_cell_marks_run_done() {
        let (repo, run_repo, clock, run_id) = setup().await;
        advance_run(
            &repo,
            &run_repo,
            &clock,
            AdvanceRunInput { run_id: run_id.clone(), edge_label: "next".into() },
        )
        .await
        .unwrap();
        let out = advance_run(
            &repo,
            &run_repo,
            &clock,
            AdvanceRunInput { run_id: run_id.clone(), edge_label: "finish".into() },
        )
        .await
        .unwrap();
        assert_eq!(out.cell_name, "end");
        assert!(out.terminal);
        assert!(out.edges.is_empty());
        let run = run_repo.get(&run_id).await.unwrap().unwrap();
        assert_eq!(run.status, RunStatus::Done);
    }

    #[tokio::test]
    async fn advance_with_unknown_edge_label_is_not_found() {
        let (repo, run_repo, clock, run_id) = setup().await;
        let err = advance_run(
            &repo,
            &run_repo,
            &clock,
            AdvanceRunInput { run_id: run_id.clone(), edge_label: "wrong".into() },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, CoreError::NotFound(_)));
    }

    #[tokio::test]
    async fn advance_on_done_run_is_not_running() {
        let (repo, run_repo, clock, run_id) = setup().await;
        advance_run(
            &repo,
            &run_repo,
            &clock,
            AdvanceRunInput { run_id: run_id.clone(), edge_label: "next".into() },
        )
        .await
        .unwrap();
        advance_run(
            &repo,
            &run_repo,
            &clock,
            AdvanceRunInput { run_id: run_id.clone(), edge_label: "finish".into() },
        )
        .await
        .unwrap();
        let err = advance_run(
            &repo,
            &run_repo,
            &clock,
            AdvanceRunInput { run_id: run_id.clone(), edge_label: "any".into() },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, CoreError::RunNotRunning));
    }

    #[tokio::test]
    async fn advance_with_loop_returns_to_same_cell() {
        let repo = InMemoryHarnessRepository::new();
        let run_repo = InMemoryRunRepository::new();
        let clock = FakeIdClock::new();
        let loop_board = BoardDefinition {
            schema_version: 1,
            start: "c1".into(),
            cells: vec![
                Cell {
                    id: "c1".into(),
                    name: "a".into(),
                    prompt: "pa".into(),
                    status: CellStatus::Active,
                    terminal: false,
                },
                Cell {
                    id: "c2".into(),
                    name: "b".into(),
                    prompt: "pb".into(),
                    status: CellStatus::Active,
                    terminal: false,
                },
                Cell {
                    id: "c3".into(),
                    name: "end".into(),
                    prompt: "done".into(),
                    status: CellStatus::Active,
                    terminal: true,
                },
            ],
            edges: vec![
                Edge { from: "c1".into(), to: "c2".into(), label: "go".into(), guard: None },
                Edge { from: "c2".into(), to: "c1".into(), label: "loop".into(), guard: None },
                Edge { from: "c2".into(), to: "c3".into(), label: "exit".into(), guard: None },
            ],
        };
        let h = create_harness(
            &repo,
            &clock,
            CreateHarnessInput { name: "loop".into(), description: None, definition: Some(loop_board) },
        )
        .await
        .unwrap();
        let s = start_run(
            &repo,
            &run_repo,
            &clock,
            StartRunInput { harness_id: h.harness_id.clone(), project_path: None },
        )
        .await
        .unwrap();
        advance_run(
            &repo,
            &run_repo,
            &clock,
            AdvanceRunInput { run_id: s.run_id.clone(), edge_label: "go".into() },
        )
        .await
        .unwrap();
        let back = advance_run(
            &repo,
            &run_repo,
            &clock,
            AdvanceRunInput { run_id: s.run_id.clone(), edge_label: "loop".into() },
        )
        .await
        .unwrap();
        assert_eq!(back.cell_name, "a");
        assert!(!back.terminal);
    }

    #[tokio::test]
    async fn advance_branching_follows_chosen_edge() {
        let repo = InMemoryHarnessRepository::new();
        let run_repo = InMemoryRunRepository::new();
        let clock = FakeIdClock::new();
        let branch_board = BoardDefinition {
            schema_version: 1,
            start: "c1".into(),
            cells: vec![
                Cell {
                    id: "c1".into(),
                    name: "decide".into(),
                    prompt: "choose".into(),
                    status: CellStatus::Active,
                    terminal: false,
                },
                Cell {
                    id: "c2".into(),
                    name: "left".into(),
                    prompt: "left branch".into(),
                    status: CellStatus::Active,
                    terminal: true,
                },
                Cell {
                    id: "c3".into(),
                    name: "right".into(),
                    prompt: "right branch".into(),
                    status: CellStatus::Active,
                    terminal: true,
                },
            ],
            edges: vec![
                Edge {
                    from: "c1".into(),
                    to: "c2".into(),
                    label: "left".into(),
                    guard: Some(Guard { expr: "score < 5".into() }),
                },
                Edge {
                    from: "c1".into(),
                    to: "c3".into(),
                    label: "right".into(),
                    guard: Some(Guard { expr: "score >= 5".into() }),
                },
            ],
        };
        let h = create_harness(
            &repo,
            &clock,
            CreateHarnessInput { name: "branch".into(), description: None, definition: Some(branch_board) },
        )
        .await
        .unwrap();
        let s = start_run(
            &repo,
            &run_repo,
            &clock,
            StartRunInput { harness_id: h.harness_id.clone(), project_path: None },
        )
        .await
        .unwrap();
        let out = advance_run(
            &repo,
            &run_repo,
            &clock,
            AdvanceRunInput { run_id: s.run_id, edge_label: "right".into() },
        )
        .await
        .unwrap();
        assert_eq!(out.cell_name, "right");
        assert!(out.terminal);
    }
}
