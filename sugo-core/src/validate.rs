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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueCode {
    DuplicateCellId,
    StartMissing,
    UnknownCellRef,
    UnreachableCell,
    NoTerminal,
    HasDraft,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub severity: Severity,
    pub code: IssueCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationReport {
    pub ok: bool,
    pub issues: Vec<ValidationIssue>,
}

fn err(code: IssueCode, message: String, cell_id: Option<String>) -> ValidationIssue {
    ValidationIssue { severity: Severity::Error, code, message, cell_id }
}

pub fn validate_board(board: &BoardDefinition) -> ValidationReport {
    let mut issues = Vec::new();

    // duplicate_cell_id
    let mut seen = HashSet::new();
    for c in &board.cells {
        if !seen.insert(c.id.as_str()) {
            issues.push(err(
                IssueCode::DuplicateCellId,
                format!("duplicate cell id: {}", c.id),
                Some(c.id.clone()),
            ));
        }
    }
    let ids: HashSet<&str> = board.cells.iter().map(|c| c.id.as_str()).collect();

    // start_missing
    if !ids.contains(board.start.as_str()) {
        issues.push(err(
            IssueCode::StartMissing,
            format!("start cell not found: {}", board.start),
            None,
        ));
    }

    // unknown_cell_ref
    for e in &board.edges {
        if !ids.contains(e.from.as_str()) {
            issues.push(err(
                IssueCode::UnknownCellRef,
                format!("edge.from unknown: {}", e.from),
                Some(e.from.clone()),
            ));
        }
        if !ids.contains(e.to.as_str()) {
            issues.push(err(
                IssueCode::UnknownCellRef,
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
                    IssueCode::UnreachableCell,
                    format!("cell unreachable from start: {}", c.id),
                    Some(c.id.clone()),
                ));
            }
        }
    }

    // no_terminal
    if !board.cells.iter().any(|c| c.terminal) {
        issues.push(err(IssueCode::NoTerminal, "no terminal cell".into(), None));
    }

    // has_draft (warning)
    for c in &board.cells {
        if c.status == CellStatus::Draft {
            issues.push(ValidationIssue {
                severity: Severity::Warning,
                code: IssueCode::HasDraft,
                message: format!("draft cell: {}", c.id),
                cell_id: Some(c.id.clone()),
            });
        }
    }

    let ok = !issues.iter().any(|i| i.severity == Severity::Error);
    ValidationReport { ok, issues }
}

pub fn validate_definition(def: &BoardDefinition) -> ValidationReport {
    validate_board(def)
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
    /// 指定 code の issue を返す（存在しなければ None）。
    fn find(r: &ValidationReport, code: IssueCode) -> Option<&ValidationIssue> {
        r.issues.iter().find(|i| i.code == code)
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
        let r = validate_board(&b);
        let issue = find(&r, IssueCode::DuplicateCellId).expect("duplicate_cell_id issue");
        assert_eq!(issue.severity, Severity::Error);
        assert!(!r.ok);
    }

    #[test]
    fn detects_start_missing() {
        let b = board(vec![cell("c1", true, CellStatus::Active)], vec![], "nope");
        let r = validate_board(&b);
        let issue = find(&r, IssueCode::StartMissing).expect("start_missing issue");
        assert_eq!(issue.severity, Severity::Error);
        assert!(!r.ok);
    }

    #[test]
    fn detects_unknown_cell_ref() {
        let b = board(
            vec![cell("c1", true, CellStatus::Active)],
            vec![edge("c1", "ghost")],
            "c1",
        );
        let r = validate_board(&b);
        let issue = find(&r, IssueCode::UnknownCellRef).expect("unknown_cell_ref issue");
        assert_eq!(issue.severity, Severity::Error);
        assert!(!r.ok);
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
        let r = validate_board(&b);
        let issue = find(&r, IssueCode::UnreachableCell).expect("unreachable_cell issue");
        assert_eq!(issue.severity, Severity::Error);
        assert!(!r.ok);
    }

    #[test]
    fn detects_no_terminal() {
        let b = board(
            vec![cell("c1", false, CellStatus::Active), cell("c2", false, CellStatus::Active)],
            vec![edge("c1", "c2")],
            "c1",
        );
        let r = validate_board(&b);
        let issue = find(&r, IssueCode::NoTerminal).expect("no_terminal issue");
        assert_eq!(issue.severity, Severity::Error);
        assert!(!r.ok);
    }

    #[test]
    fn reports_has_draft_as_warning() {
        let b = board(
            vec![cell("c1", false, CellStatus::Active), cell("c2", true, CellStatus::Draft)],
            vec![edge("c1", "c2")],
            "c1",
        );
        let r = validate_board(&b);
        let draft = find(&r, IssueCode::HasDraft).expect("has_draft issue");
        assert_eq!(draft.severity, Severity::Warning);
        assert!(r.ok); // warning のみなら ok=true
    }

    #[test]
    fn issue_code_serializes_to_snake_case_json() {
        // JSON 出力形が設計の固定文字列集合と一致することを固定する。
        let pairs = [
            (IssueCode::DuplicateCellId, "\"duplicate_cell_id\""),
            (IssueCode::StartMissing, "\"start_missing\""),
            (IssueCode::UnknownCellRef, "\"unknown_cell_ref\""),
            (IssueCode::UnreachableCell, "\"unreachable_cell\""),
            (IssueCode::NoTerminal, "\"no_terminal\""),
            (IssueCode::HasDraft, "\"has_draft\""),
        ];
        for (code, json) in pairs {
            assert_eq!(serde_json::to_string(&code).unwrap(), json);
        }
    }

    #[test]
    fn validate_definition_wraps_validate_board() {
        let b = board(
            vec![cell("c1", false, CellStatus::Active), cell("c2", true, CellStatus::Active)],
            vec![edge("c1", "c2")],
            "c1",
        );
        assert_eq!(validate_definition(&b), validate_board(&b));
    }
}
