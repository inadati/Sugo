//! Output ports: traits the core drives and adapters implement.
//!
//! Following the hexagonal design, the core depends only on these traits, not
//! on concrete IO. [`id_clock::IdClock`] supplies id and timestamp generation,
//! and [`repository::HarnessRepository`] is the persistence port implemented by
//! infrastructure adapters (e.g. SQLite) and by in-memory fakes in tests.

pub mod id_clock;
pub mod repository;
pub mod run_repository;
