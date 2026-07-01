//! The board definition: the immutable, serializable shape of a harness.

use super::cell::Cell;
use super::edge::Edge;
use serde::{Deserialize, Serialize};

/// The full graph of a harness board: cells, edges, and start cell.
///
/// A `BoardDefinition` is immutable: an edit never mutates an existing
/// definition but produces a new [`BoardVersion`](super::harness::BoardVersion)
/// carrying a fresh definition. It is the unit serialized to and from
/// `board_versions.definition_json`, and the input over which validation runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct BoardDefinition {
    /// Schema version of the definition format.
    pub schema_version: u32,
    /// Id of the cell where the board begins.
    pub start: String,
    /// All cells on the board.
    pub cells: Vec<Cell>,
    /// All directed transitions between cells.
    pub edges: Vec<Edge>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::cell::{Cell, CellStatus};

    #[test]
    fn board_json_roundtrip() {
        let board = BoardDefinition {
            schema_version: 1,
            start: "c1".into(),
            cells: vec![Cell {
                id: "c1".into(),
                name: "intro".into(),
                prompt: "hi".into(),
                status: CellStatus::Active,
                terminal: false,
                request_memo: "".into(),
            }],
            edges: vec![],
        };
        let json = serde_json::to_string(&board).unwrap();
        let back: BoardDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(board, back);
    }

    #[test]
    fn board_json_roundtrip_rich() {
        // Exercises the full shape: multiple cells, Draft status, edges with
        // Guard=Some and Guard=None, to confirm none of these optional fields
        // are silently dropped or corrupted through a serialization round-trip.
        use crate::domain::cell::CellStatus;
        use crate::domain::edge::{Edge, Guard};
        let board = BoardDefinition {
            schema_version: 1,
            start: "c1".into(),
            cells: vec![
                Cell {
                    id: "c1".into(),
                    name: "intro".into(),
                    prompt: "hello".into(),
                    status: CellStatus::Active,
                    terminal: false,
                    request_memo: "".into(),
                },
                Cell {
                    id: "c2".into(),
                    name: "wip".into(),
                    prompt: "work".into(),
                    status: CellStatus::Draft,
                    terminal: false,
                    request_memo: "".into(),
                },
                Cell {
                    id: "c3".into(),
                    name: "done".into(),
                    prompt: "".into(),
                    status: CellStatus::Active,
                    terminal: true,
                    request_memo: "".into(),
                },
            ],
            edges: vec![
                Edge {
                    from: "c1".into(),
                    to: "c2".into(),
                    label: "next".into(),
                    guard: Some(Guard { expr: "score > 0".into() }),
                },
                Edge {
                    from: "c2".into(),
                    to: "c3".into(),
                    label: "finish".into(),
                    guard: None,
                },
            ],
        };
        let json = serde_json::to_string(&board).unwrap();
        let back: BoardDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(board, back);
        // Draft status must survive the round-trip (not normalized to Active).
        assert_eq!(back.cells[1].status, CellStatus::Draft);
        // Guard=Some must survive without becoming None.
        assert!(back.edges[0].guard.is_some());
        assert_eq!(back.edges[0].guard.as_ref().unwrap().expr, "score > 0");
        // Guard=None must remain None.
        assert!(back.edges[1].guard.is_none());
    }
}
