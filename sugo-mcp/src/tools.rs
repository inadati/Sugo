//! Tool argument types and the production [`IdClock`] implementation.
//!
//! These types form the MCP input boundary: each derives [`JsonSchema`] so
//! rmcp can publish a schema, and [`Deserialize`] so rmcp can parse the
//! incoming JSON arguments. `BoardDefinition`'s schema is provided by the
//! `schema` feature of `sugo-core`.

use schemars::JsonSchema;
use serde::Deserialize;
use sugo_core::domain::board::BoardDefinition;
use sugo_core::ports::id_clock::IdClock;

/// Production clock/ID source: random UUIDs and wall-clock timestamps.
pub struct RealIdClock;

impl IdClock for RealIdClock {
    fn new_id(&self) -> String {
        uuid::Uuid::new_v4().to_string()
    }

    fn now_iso(&self) -> String {
        chrono::Local::now().to_rfc3339()
    }
}

/// Arguments for `sugo_create_harness`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateArgs {
    /// Display name of the new harness.
    pub name: String,
    /// Optional free-text description shown in harness listings.
    #[serde(default)]
    pub description: Option<String>,
    /// Optional board definition. When omitted a minimal template is used.
    #[serde(default)]
    pub definition: Option<BoardDefinition>,
}

/// Arguments for `sugo_set_description`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetDescriptionArgs {
    /// Target harness id.
    pub harness_id: String,
    /// New description text. Pass null to clear the description.
    #[serde(default)]
    pub description: Option<String>,
}

/// Arguments for `sugo_delete_harness`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteHarnessArgs {
    /// Target harness id.
    pub harness_id: String,
}

/// Arguments for `sugo_status`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct StatusArgs {
    /// Target harness id. When omitted, a summary of all harnesses is returned.
    #[serde(default)]
    pub harness_id: Option<String>,
}

/// Arguments for `sugo_edit_cell`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct EditArgs {
    /// Target harness id.
    pub harness_id: String,
    /// Id of the cell whose prompt is replaced.
    pub cell_id: String,
    /// New prompt text for the cell.
    pub prompt: String,
    /// Optimistic-lock version the caller expects to edit against.
    pub expected_lock_version: i64,
}

/// Arguments for `sugo_get_cell`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetCellArgs {
    /// Target harness id.
    pub harness_id: String,
    /// Id of the cell to read in full (including its current prompt).
    pub cell_id: String,
}

/// Arguments for `sugo_validate_harness`.
///
/// Either a stored harness is validated via `harness_id`, or a board
/// `definition` is validated directly. Exactly one must be supplied.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ValidateArgs {
    /// Target harness id. Mutually exclusive with `definition`.
    #[serde(default)]
    pub harness_id: Option<String>,
    /// Board definition to validate directly. Mutually exclusive with `harness_id`.
    #[serde(default)]
    pub definition: Option<BoardDefinition>,
}

/// Arguments for `sugo_start`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct StartArgs {
    /// Id of the harness to run.
    pub harness_id: String,
    /// Absolute path of the project directory. Required: Nipper-linked runs route by it.
    pub project_path: String,
}

/// Arguments for `sugo_advance`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AdvanceArgs {
    /// Id of the run to advance.
    pub run_id: String,
    /// Edge label to follow from the current cell.
    pub edge_label: String,
}

/// Per-cell change in a `sugo_update_harness` call.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CellChangeArgs {
    /// Id of the cell to update.
    pub cell_id: String,
    /// New prompt text; omit to keep the current prompt.
    #[serde(default)]
    pub prompt: Option<String>,
    /// New status: "active" or "draft"; omit to keep the current status.
    #[serde(default)]
    pub status: Option<String>,
    /// New request_memo text; omit to keep the current memo. Pass "" to clear.
    #[serde(default)]
    pub memo: Option<String>,
}

/// Identifies an edge to remove by its (from, to, label) triple.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct EdgeKeyArgs {
    pub from: String,
    pub to: String,
    pub label: String,
}

/// An edge to add in a `sugo_update_harness` call.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct EdgeAddArgs {
    pub from: String,
    pub to: String,
    pub label: String,
    /// Optional branch condition expression.
    #[serde(default)]
    pub guard: Option<String>,
}

/// A new cell to add in a `sugo_update_harness` call.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CellAddArgs {
    /// Board-unique identifier for the new cell. Caller-specified (e.g. a UUID);
    /// must not collide with an existing cell id or another cell_add entry in
    /// the same call.
    pub id: String,
    /// Human-readable label for the new cell.
    pub name: String,
    /// Prompt text for the new cell.
    pub prompt: String,
    /// "active" or "draft"; required (no default).
    pub status: String,
    /// Whether the new cell terminates the board.
    pub terminal: bool,
}

/// Arguments for `sugo_update_harness`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateArgs {
    /// Target harness id.
    pub harness_id: String,
    /// Optimistic-lock version the caller expects to edit against.
    pub expected_lock_version: i64,
    /// Per-cell changes (prompt and/or status); defaults to empty.
    #[serde(default)]
    pub cell_changes: Vec<CellChangeArgs>,
    /// New cells to add; defaults to empty. Every field within each entry is
    /// required (no per-field defaults): id, name, prompt, status, terminal.
    #[serde(default)]
    pub cell_add: Vec<CellAddArgs>,
    /// Edges to add; defaults to empty.
    #[serde(default)]
    pub edge_add: Vec<EdgeAddArgs>,
    /// Edges to remove (matched by from+to+label); defaults to empty.
    #[serde(default)]
    pub edge_remove: Vec<EdgeKeyArgs>,
    /// Cell ids to remove; defaults to empty. The board's start cell cannot be
    /// removed (rejected with a validation_failed error carrying a
    /// cannot_remove_start_cell issue). Removing a cell also removes any edge
    /// connected to it. A missing cell id is silently ignored (idempotent),
    /// matching edge_remove's behavior.
    #[serde(default)]
    pub cell_remove: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_id_clock_produces_unique_ids() {
        let clock = RealIdClock;
        let a = clock.new_id();
        let b = clock.new_id();
        assert_ne!(a, b);
        assert_eq!(a.len(), 36); // UUID v4 hyphenated form
    }

    #[test]
    fn real_id_clock_now_is_rfc3339() {
        let clock = RealIdClock;
        let now = clock.now_iso();
        assert!(chrono::DateTime::parse_from_rfc3339(&now).is_ok());
    }

    #[test]
    fn create_args_defaults_definition_to_none() {
        let args: CreateArgs = serde_json::from_str(r#"{"name":"h"}"#).unwrap();
        assert_eq!(args.name, "h");
        assert!(args.definition.is_none());
    }

    #[test]
    fn edit_args_round_trip() {
        let args: EditArgs = serde_json::from_str(
            r#"{"harness_id":"h1","cell_id":"c1","prompt":"p","expected_lock_version":3}"#,
        )
        .unwrap();
        assert_eq!(args.harness_id, "h1");
        assert_eq!(args.cell_id, "c1");
        assert_eq!(args.prompt, "p");
        assert_eq!(args.expected_lock_version, 3);
    }

    #[test]
    fn edit_args_missing_field_errors() {
        // expected_lock_version is required; omitting it is a deserialize error.
        let res: Result<EditArgs, _> =
            serde_json::from_str(r#"{"harness_id":"h1","cell_id":"c1","prompt":"p"}"#);
        assert!(res.is_err());
    }

    #[test]
    fn status_args_with_harness_id() {
        let args: StatusArgs = serde_json::from_str(r#"{"harness_id":"h1"}"#).unwrap();
        assert_eq!(args.harness_id.as_deref(), Some("h1"));
    }

    #[test]
    fn status_args_omitting_harness_id_defaults_to_none() {
        // harness_id is optional; the empty object selects the all-summaries path.
        let args: StatusArgs = serde_json::from_str(r#"{}"#).unwrap();
        assert!(args.harness_id.is_none());
    }

    #[test]
    fn validate_args_with_harness_id() {
        let args: ValidateArgs = serde_json::from_str(r#"{"harness_id":"h1"}"#).unwrap();
        assert_eq!(args.harness_id.as_deref(), Some("h1"));
        assert!(args.definition.is_none());
    }

    #[test]
    fn validate_args_with_definition() {
        let json = r#"{"definition":{"schema_version":1,"start":"c1",
            "cells":[{"id":"c1","name":"c1","prompt":"p","status":"active","terminal":true}],
            "edges":[]}}"#;
        let args: ValidateArgs = serde_json::from_str(json).unwrap();
        assert!(args.harness_id.is_none());
        let def = args.definition.expect("definition present");
        assert_eq!(def.start, "c1");
        assert_eq!(def.cells.len(), 1);
    }

    #[test]
    fn validate_args_empty_object_leaves_both_none() {
        // Neither field supplied; the boundary then reports an error at call time.
        let args: ValidateArgs = serde_json::from_str(r#"{}"#).unwrap();
        assert!(args.harness_id.is_none());
        assert!(args.definition.is_none());
    }

    #[test]
    fn start_args_requires_project_path() {
        let res: Result<StartArgs, _> = serde_json::from_str(r#"{"harness_id":"h1"}"#);
        assert!(res.is_err());
    }

    #[test]
    fn start_args_round_trip() {
        let args: StartArgs =
            serde_json::from_str(r#"{"harness_id":"h1","project_path":"/abs/p"}"#).unwrap();
        assert_eq!(args.harness_id, "h1");
        assert_eq!(args.project_path, "/abs/p");
    }

    #[test]
    fn advance_args_round_trip() {
        let args: AdvanceArgs =
            serde_json::from_str(r#"{"run_id":"r1","edge_label":"next"}"#).unwrap();
        assert_eq!(args.run_id, "r1");
        assert_eq!(args.edge_label, "next");
    }

    #[test]
    fn update_args_cell_changes_default_to_empty() {
        let args: UpdateArgs =
            serde_json::from_str(r#"{"harness_id":"h1","expected_lock_version":3}"#).unwrap();
        assert_eq!(args.harness_id, "h1");
        assert_eq!(args.expected_lock_version, 3);
        assert!(args.cell_changes.is_empty());
        assert!(args.cell_add.is_empty());
        assert!(args.edge_add.is_empty());
        assert!(args.edge_remove.is_empty());
    }

    #[test]
    fn update_args_full_round_trip() {
        let json = r#"{
            "harness_id": "h1",
            "expected_lock_version": 2,
            "cell_changes": [{"cell_id":"c1","prompt":"new","status":"active"}],
            "cell_add": [{"id":"c9","name":"ninth","prompt":"do ninth","status":"draft","terminal":false}],
            "edge_add": [{"from":"c1","to":"c2","label":"next"}],
            "edge_remove": [{"from":"c2","to":"c3","label":"old"}]
        }"#;
        let args: UpdateArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.cell_changes[0].cell_id, "c1");
        assert_eq!(args.cell_changes[0].prompt.as_deref(), Some("new"));
        assert_eq!(args.cell_changes[0].status.as_deref(), Some("active"));
        assert_eq!(args.cell_add[0].id, "c9");
        assert_eq!(args.cell_add[0].name, "ninth");
        assert_eq!(args.cell_add[0].prompt, "do ninth");
        assert_eq!(args.cell_add[0].status, "draft");
        assert!(!args.cell_add[0].terminal);
        assert_eq!(args.edge_add[0].from, "c1");
        assert_eq!(args.edge_remove[0].label, "old");
    }

    #[test]
    fn cell_change_args_all_optional_fields_default_to_none() {
        let args: CellChangeArgs = serde_json::from_str(r#"{"cell_id":"c1"}"#).unwrap();
        assert_eq!(args.cell_id, "c1");
        assert!(args.prompt.is_none());
        assert!(args.status.is_none());
    }

    #[test]
    fn edge_add_args_guard_defaults_to_none() {
        let args: EdgeAddArgs =
            serde_json::from_str(r#"{"from":"c1","to":"c2","label":"next"}"#).unwrap();
        assert!(args.guard.is_none());
    }

    #[test]
    fn cell_add_args_missing_status_is_deserialize_error() {
        // status has no default — omitting any required field must be a
        // deserialize error, unlike CellChangeArgs's optional fields.
        let res: Result<CellAddArgs, _> =
            serde_json::from_str(r#"{"id":"c9","name":"ninth","prompt":"p","terminal":false}"#);
        assert!(res.is_err());
    }

    #[test]
    fn cell_add_args_missing_id_is_deserialize_error() {
        // id has no default — omitting it must be a deserialize error.
        let res: Result<CellAddArgs, _> = serde_json::from_str(
            r#"{"name":"ninth","prompt":"p","status":"active","terminal":false}"#,
        );
        assert!(res.is_err());
    }

    #[test]
    fn cell_add_args_missing_name_is_deserialize_error() {
        // name has no default — omitting it must be a deserialize error.
        let res: Result<CellAddArgs, _> =
            serde_json::from_str(r#"{"id":"c9","prompt":"p","status":"active","terminal":false}"#);
        assert!(res.is_err());
    }

    #[test]
    fn cell_add_args_missing_prompt_is_deserialize_error() {
        // prompt has no default — omitting it must be a deserialize error.
        let res: Result<CellAddArgs, _> = serde_json::from_str(
            r#"{"id":"c9","name":"ninth","status":"active","terminal":false}"#,
        );
        assert!(res.is_err());
    }

    #[test]
    fn cell_add_args_missing_terminal_is_deserialize_error() {
        // terminal has no default — omitting it must be a deserialize error.
        let res: Result<CellAddArgs, _> =
            serde_json::from_str(r#"{"id":"c9","name":"ninth","prompt":"p","status":"active"}"#);
        assert!(res.is_err());
    }
}
