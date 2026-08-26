//! Structural validation of board definitions.
//!
//! Provides [`validate_board`] and its [`ValidationReport`]/[`Severity`] types,
//! which inspect a [`BoardDefinition`] for structural problems (missing
//! start/terminal cells, unreachable cells, dangling edges, etc.) so callers can
//! reject or surface invalid harnesses before they are run.

use crate::domain::board::BoardDefinition;
use crate::domain::cell::CellStatus;
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};

/// Severity of a single validation issue.
///
/// `Error` issues make the board invalid (`ValidationReport::ok == false`);
/// `Warning` issues are advisory and do not flip `ok` to `false`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Blocking problem; the board is considered invalid.
    Error,
    /// Advisory problem; the board is still considered valid.
    Warning,
}

/// Stable machine-readable code identifying a category of validation issue.
///
/// Each variant serializes to the snake_case string used in the MCP I/O
/// contract (see the P1 design doc).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueCode {
    /// Two or more cells share the same `id`.
    DuplicateCellId,
    /// `board.start` does not reference any existing cell.
    StartMissing,
    /// An edge endpoint (`from`/`to`) references an unknown cell id.
    UnknownCellRef,
    /// A cell cannot be reached from `start` by following valid edges.
    UnreachableCell,
    /// The board has no terminal cell, so a run could never finish.
    NoTerminal,
    /// At least one cell is still in draft status (advisory warning).
    HasDraft,
    /// `cell_remove` targeted the board's `start` cell, which cannot be removed.
    CannotRemoveStartCell,
    /// フォルダ名が空、または64文字を超えている。
    InvalidFolderName,
}

/// A single problem found while validating a board definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationIssue {
    /// Whether this issue blocks the board (`Error`) or is advisory (`Warning`).
    pub severity: Severity,
    /// Stable code identifying the category of this issue.
    pub code: IssueCode,
    /// Human-readable description of the problem.
    pub message: String,
    /// The cell this issue concerns, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell_id: Option<String>,
}

/// The outcome of validating a board definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationReport {
    /// `true` when there are no `Error`-severity issues. Note the non-obvious
    /// rule: a report with only `Warning` issues still has `ok == true`; any
    /// `Error` issue sets `ok == false`.
    pub ok: bool,
    /// All issues found, in detection order (errors and warnings interleaved).
    pub issues: Vec<ValidationIssue>,
}

fn err(code: IssueCode, message: String, cell_id: Option<String>) -> ValidationIssue {
    ValidationIssue { severity: Severity::Error, code, message, cell_id }
}

/// Validate a board definition and return all detected issues.
///
/// Runs the following structural checks: duplicate cell ids, missing `start`
/// cell, unknown edge endpoints, cells unreachable from `start` (via a BFS
/// over valid edges that is cycle-safe), absence of a terminal cell, and draft
/// cells (reported as warnings). The returned [`ValidationReport::ok`] is
/// `true` unless at least one `Error`-severity issue was found.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::board::BoardDefinition;
    use crate::domain::cell::{Cell, CellStatus};
    use crate::domain::edge::Edge;

    fn cell(id: &str, terminal: bool, status: CellStatus) -> Cell {
        Cell { id: id.into(), name: id.into(), prompt: "p".into(), status, terminal, request_memo: "".into() }
    }
    fn edge(from: &str, to: &str) -> Edge {
        Edge { from: from.into(), to: to.into(), label: "l".into(), guard: None }
    }
    fn board(cells: Vec<Cell>, edges: Vec<Edge>, start: &str) -> BoardDefinition {
        BoardDefinition { schema_version: 1, start: start.into(), cells, edges }
    }
    /// Return the issue with the given code, or None if absent.
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
        assert!(r.ok); // warning-only report is still ok=true
    }

    #[test]
    fn issue_code_serializes_to_snake_case_json() {
        // Pin the JSON output to the design's fixed string set.
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
    fn reachability_handles_cycle_without_infinite_loop() {
        // c1 -> c2 -> c1 forms a cycle; c1 -> c3 reaches the terminal c3.
        // The BFS must terminate (cycle-safe) and report no unreachable cells.
        let b = board(
            vec![
                cell("c1", false, CellStatus::Active),
                cell("c2", false, CellStatus::Active),
                cell("c3", true, CellStatus::Active),
            ],
            vec![edge("c1", "c2"), edge("c2", "c1"), edge("c1", "c3")],
            "c1",
        );
        let r = validate_board(&b);
        assert!(find(&r, IssueCode::UnreachableCell).is_none(), "no cell should be unreachable in a cycle");
        assert!(r.ok);
    }

    #[test]
    fn reachability_handles_diamond_branch() {
        // Diamond: c1 -> c2, c1 -> c3, c2 -> c4, c3 -> c4. c4 is reached via two
        // paths; the BFS must mark every cell reachable without misjudging the
        // doubly-reached c4.
        let b = board(
            vec![
                cell("c1", false, CellStatus::Active),
                cell("c2", false, CellStatus::Active),
                cell("c3", false, CellStatus::Active),
                cell("c4", true, CellStatus::Active),
            ],
            vec![edge("c1", "c2"), edge("c1", "c3"), edge("c2", "c4"), edge("c3", "c4")],
            "c1",
        );
        let r = validate_board(&b);
        assert!(find(&r, IssueCode::UnreachableCell).is_none(), "all diamond cells should be reachable");
        assert!(r.ok);
    }

    #[test]
    fn reachability_handles_multi_hop_path() {
        // Linear multi-hop path c1 -> c2 -> c3 -> c4 (terminal): every cell is
        // reachable from start across several indirect hops.
        let b = board(
            vec![
                cell("c1", false, CellStatus::Active),
                cell("c2", false, CellStatus::Active),
                cell("c3", false, CellStatus::Active),
                cell("c4", true, CellStatus::Active),
            ],
            vec![edge("c1", "c2"), edge("c2", "c3"), edge("c3", "c4")],
            "c1",
        );
        let r = validate_board(&b);
        assert!(find(&r, IssueCode::UnreachableCell).is_none(), "all cells on the path should be reachable");
        assert!(r.ok);
    }
}
