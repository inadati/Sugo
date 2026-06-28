//! Application use cases: the IO-free orchestration layer over the domain.
//!
//! Each submodule is one harness operation expressed against the
//! [`HarnessRepository`](crate::ports::repository::HarnessRepository) port:
//! [`create_harness`] seeds a new harness with its initial board version,
//! [`edit_cell`] records an edit as a fresh immutable board version under
//! optimistic locking, [`validate_harness`] runs structural checks, and
//! [`get_status`] reports current state plus the draft difference. Use cases
//! depend only on domain types and ports, never on concrete infrastructure.

pub mod advance_run;
pub mod create_harness;
pub mod edit_cell;
pub mod get_status;
pub mod lease;
pub mod start_run;
pub mod update_harness;
pub mod validate_harness;
