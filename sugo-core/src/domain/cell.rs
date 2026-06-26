//! The cell (マス): a single square on the harness board.

use serde::{Deserialize, Serialize};

/// Lifecycle state of a [`Cell`].
///
/// `Active` cells are part of the live board; `Draft` cells are proposed but
/// not yet promoted, and surface in the status `draft_diff`. Serializes
/// lowercase (`active` / `draft`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum CellStatus {
    /// Live cell included in the active board.
    Active,
    /// Proposed cell not yet promoted; reported as a draft difference.
    Draft,
}

/// A square on the harness board: one prompt-bearing step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Cell {
    /// Board-unique identifier referenced by edges and the board `start`.
    pub id: String,
    /// Human-readable label for the cell.
    pub name: String,
    /// Prompt text driving the agent at this cell; the unit `edit_cell` edits.
    pub prompt: String,
    /// Whether the cell is active or a draft.
    pub status: CellStatus,
    /// Whether the cell terminates the board (a valid end state).
    pub terminal: bool,
}
