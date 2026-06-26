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
    /// Optional board definition. When omitted a minimal template is used.
    #[serde(default)]
    pub definition: Option<BoardDefinition>,
}

/// Arguments for `sugo_status`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct StatusArgs {
    /// Target harness id.
    pub harness_id: String,
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

/// Arguments for `sugo_validate_harness`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ValidateArgs {
    /// Target harness id.
    pub harness_id: String,
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
}
