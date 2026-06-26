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
            }],
            edges: vec![],
        };
        let json = serde_json::to_string(&board).unwrap();
        let back: BoardDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(board, back);
    }
}
