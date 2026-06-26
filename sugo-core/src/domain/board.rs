use super::cell::Cell;
use super::edge::Edge;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct BoardDefinition {
    pub schema_version: u32,
    pub start: String,
    pub cells: Vec<Cell>,
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
