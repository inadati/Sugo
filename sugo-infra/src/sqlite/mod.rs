//! SQLite adapter implementing the harness persistence port.
//!
//! [`SqliteHarnessRepository`] stores harness heads and their immutable board
//! versions, preserving per-harness monotonic `version_no` and enforcing
//! optimistic locking on edits. Use [`SqliteHarnessRepository::open`] for a
//! file-backed database or [`SqliteHarnessRepository::in_memory`] for an
//! ephemeral one (handy in tests).

pub mod repository;
pub mod run_repository;
pub mod schema;
/// SQLite-backed implementation of the harness repository port. See its
/// `open` and `in_memory` constructors to obtain an instance.
pub use repository::SqliteHarnessRepository;
pub use run_repository::SqliteRunRepository;
