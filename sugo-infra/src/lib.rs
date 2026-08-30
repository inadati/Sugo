//! Sugo infrastructure: driven-side adapters for the [`sugo_core`] ports.
//!
//! This crate provides concrete implementations of the core output ports. In
//! P1 that is the [`sqlite`] adapter, whose
//! [`SqliteHarnessRepository`](sqlite::SqliteHarnessRepository) persists
//! harnesses and their immutable board versions to SQLite. The core depends on
//! the port traits, not on this crate.

pub mod jsonl_watcher;
pub mod paths;
pub mod sqlite;
