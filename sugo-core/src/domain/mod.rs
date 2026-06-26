//! Harness domain entities.
//!
//! These types model the harness (双六盤): a [`board::BoardDefinition`] holds
//! the immutable graph of [`cell::Cell`]s and [`edge::Edge`]s, while a
//! [`harness::Harness`] tracks the live, mutable head (current version, draft
//! flag, optimistic lock) and a [`harness::BoardVersion`] is one immutable
//! snapshot of that board. The types are pure data with no IO.

pub mod board;
pub mod cell;
pub mod edge;
pub mod harness;
