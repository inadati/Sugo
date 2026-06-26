//! Board edges and their branch guards.

use serde::{Deserialize, Serialize};

/// A branch condition on an [`Edge`].
///
/// In P1 a guard is carried and validated as structure only; it is not
/// evaluated. Evaluation (and routing) arrives in P2, where routing is the
/// agent's self-reported choice rather than an engine-evaluated predicate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Guard {
    /// Raw guard expression text, preserved verbatim in P1.
    pub expr: String,
}

/// A directed transition between two [`Cell`](super::cell::Cell)s.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Edge {
    /// Source cell id.
    pub from: String,
    /// Destination cell id.
    pub to: String,
    /// Human-readable label for the transition.
    pub label: String,
    /// Optional branch condition; structure-only in P1 (see [`Guard`]).
    #[serde(default)]
    pub guard: Option<Guard>,
}
