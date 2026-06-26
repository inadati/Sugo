use crate::domain::board::BoardDefinition;
use crate::domain::cell::CellStatus;
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationReport {
    pub ok: bool,
    pub issues: Vec<ValidationIssue>,
}

fn err(code: &str, message: String, cell_id: Option<String>) -> ValidationIssue {
    ValidationIssue { severity: Severity::Error, code: code.into(), message, cell_id }
}

pub fn validate_board(board: &BoardDefinition) -> ValidationReport {
    let mut issues = Vec::new();

    // duplicate_cell_id
    let mut seen = HashSet::new();
    for c in &board.cells {
        if !seen.insert(c.id.as_str()) {
            issues.push(err(
                "duplicate_cell_id",
                format!("duplicate cell id: {}", c.id),
                Some(c.id.clone()),
            ));
        }
    }
    let ids: HashSet<&str> = board.cells.iter().map(|c| c.id.as_str()).collect();

    // start_missing
    if !ids.contains(board.start.as_str()) {
        issues.push(err(
            "start_missing",
            format!("start cell not found: {}", board.start),
            None,
        ));
    }

    // unknown_cell_ref
    for e in &board.edges {
        if !ids.contains(e.from.as_str()) {
            issues.push(err(
                "unknown_cell_ref",
                format!("edge.from unknown: {}", e.from),
                Some(e.from.clone()),
            ));
        }
        if !ids.contains(e.to.as_str()) {
            issues.push(err(
                "unknown_cell_ref",
                format!("edge.to unknown: {}", e.to),
                Some(e.to.clone()),
            ));
        }
    }

    // unreachable_cell (BFS from start over valid edges)
    if ids.contains(board.start.as_str()) {
        let mut reachable = HashSet::new();
        let mut queue = VecDeque::new();
        reachable.insert(board.start.as_str());
        queue.push_back(board.start.as_str());
        while let Some(cur) = queue.pop_front() {
            for e in &board.edges {
                if e.from == cur && ids.contains(e.to.as_str()) && reachable.insert(e.to.as_str()) {
                    queue.push_back(e.to.as_str());
                }
            }
        }
        for c in &board.cells {
            if !reachable.contains(c.id.as_str()) {
                issues.push(err(
                    "unreachable_cell",
                    format!("cell unreachable from start: {}", c.id),
                    Some(c.id.clone()),
                ));
            }
        }
    }

    // no_terminal
    if !board.cells.iter().any(|c| c.terminal) {
        issues.push(err("no_terminal", "no terminal cell".into(), None));
    }

    // has_draft (warning)
    for c in &board.cells {
        if c.status == CellStatus::Draft {
            issues.push(ValidationIssue {
                severity: Severity::Warning,
                code: "has_draft".into(),
                message: format!("draft cell: {}", c.id),
                cell_id: Some(c.id.clone()),
            });
        }
    }

    let ok = !issues.iter().any(|i| i.severity == Severity::Error);
    ValidationReport { ok, issues }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::board::BoardDefinition;
    use crate::domain::cell::{Cell, CellStatus};
    use crate::domain::edge::Edge;

    fn cell(id: &str, terminal: bool, status: CellStatus) -> Cell {
        Cell { id: id.into(), name: id.into(), prompt: "p".into(), status, terminal }
    }
    fn edge(from: &str, to: &str) -> Edge {
        Edge { from: from.into(), to: to.into(), label: "l".into(), guard: None }
    }
    fn board(cells: Vec<Cell>, edges: Vec<Edge>, start: &str) -> BoardDefinition {
        BoardDefinition { schema_version: 1, start: start.into(), cells, edges }
    }
    fn codes(r: &ValidationReport) -> Vec<String> {
        r.issues.iter().map(|i| i.code.clone()).collect()
    }

    #[test]
    fn valid_board_is_ok() {
        let b = board(
            vec![cell("c1", false, CellStatus::Active), cell("c2", true, CellStatus::Active)],
            vec![edge("c1", "c2")],
            "c1",
        );
        let r = validate_board(&b);
        assert!(r.ok);
        assert!(r.issues.is_empty());
    }

    #[test]
    fn detects_duplicate_cell_id() {
        let b = board(
            vec![cell("c1", true, CellStatus::Active), cell("c1", false, CellStatus::Active)],
            vec![],
            "c1",
        );
        assert!(codes(&validate_board(&b)).contains(&"duplicate_cell_id".to_string()));
    }

    #[test]
    fn detects_start_missing() {
        let b = board(vec![cell("c1", true, CellStatus::Active)], vec![], "nope");
        assert!(codes(&validate_board(&b)).contains(&"start_missing".to_string()));
    }

    #[test]
    fn detects_unknown_cell_ref() {
        let b = board(
            vec![cell("c1", true, CellStatus::Active)],
            vec![edge("c1", "ghost")],
            "c1",
        );
        assert!(codes(&validate_board(&b)).contains(&"unknown_cell_ref".to_string()));
    }

    #[test]
    fn detects_unreachable_cell() {
        let b = board(
            vec![
                cell("c1", true, CellStatus::Active),
                cell("c2", true, CellStatus::Active),
            ],
            vec![],
            "c1",
        );
        assert!(codes(&validate_board(&b)).contains(&"unreachable_cell".to_string()));
    }

    #[test]
    fn detects_no_terminal() {
        let b = board(
            vec![cell("c1", false, CellStatus::Active), cell("c2", false, CellStatus::Active)],
            vec![edge("c1", "c2")],
            "c1",
        );
        assert!(codes(&validate_board(&b)).contains(&"no_terminal".to_string()));
    }

    #[test]
    fn reports_has_draft_as_warning() {
        let b = board(
            vec![cell("c1", false, CellStatus::Active), cell("c2", true, CellStatus::Draft)],
            vec![edge("c1", "c2")],
            "c1",
        );
        let r = validate_board(&b);
        let draft = r.issues.iter().find(|i| i.code == "has_draft").unwrap();
        assert_eq!(draft.severity, Severity::Warning);
        assert!(r.ok); // warning のみなら ok=true
    }
}
