//! Sugo core: the pure domain library for the harness (双六盤) data model.
//!
//! Sugo lets an agent assemble an AI harness like a sugoroku board. This crate
//! is the IO-free hexagonal core that every other layer depends on: it defines
//! the harness data model, the output ports (traits) it drives, and the
//! usecases that operate on them. It knows nothing about SQLite, MCP, or any
//! concrete IO; infrastructure adapters implement its [`ports`] and the MCP
//! layer calls its [`usecase`] functions.
//!
//! # Layout
//!
//! - [`domain`]: harness entities ([`Cell`](domain::cell::Cell),
//!   [`Edge`](domain::edge::Edge), [`BoardDefinition`](domain::board::BoardDefinition),
//!   [`Harness`](domain::harness::Harness),
//!   [`BoardVersion`](domain::harness::BoardVersion)).
//! - [`ports`]: output-port traits the core drives, implemented by adapters.
//! - [`usecase`]: application operations (create/edit/validate/status).
//! - [`validate`]: structural validation producing an issue list.
//! - [`error`]: the typed [`CoreError`](error::CoreError) returned by usecases.
//!
//! # P1 scope
//!
//! Edits are recorded as immutable `board_version`s under optimistic locking;
//! progression, runs, and GUI concerns are deferred to later phases.

#[cfg(any(test, feature = "test-support"))]
pub mod contract;
pub mod domain;
pub mod error;
pub mod ports;
pub mod usecase;
pub mod validate;
