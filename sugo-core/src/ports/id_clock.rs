//! The id/clock output port.

/// Source of identifiers and timestamps for the core.
///
/// Abstracting these behind a port keeps usecases pure and deterministic in
/// tests: production supplies real UUIDs and wall-clock time, while fakes can
/// return fixed values.
pub trait IdClock: Send + Sync {
    /// Returns a fresh unique identifier (e.g. for a harness or board version).
    fn new_id(&self) -> String;
    /// Returns the current timestamp as an ISO 8601 string.
    fn now_iso(&self) -> String;
}
