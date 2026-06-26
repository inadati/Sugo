//! The harness head and its immutable board-version snapshots.

use super::board::BoardDefinition;

/// The mutable head of a harness: it points at the current board version and
/// carries the optimistic lock.
///
/// A `Harness` is the live record; the board content itself lives in immutable
/// [`BoardVersion`]s. Each successful edit appends a new version and bumps both
/// `current_version` and `lock_version`.
#[derive(Debug, Clone, PartialEq)]
pub struct Harness {
    /// Unique harness identifier.
    pub id: String,
    /// Human-readable harness name.
    pub name: String,
    /// `version_no` of the board version currently serving as head.
    pub current_version: i64,
    /// Whether the current board contains any draft cell.
    pub has_draft: bool,
    /// Optimistic-lock counter; a stale value on edit yields a lock conflict.
    pub lock_version: i64,
    /// Creation timestamp (ISO 8601).
    pub created_at: String,
    /// Last-update timestamp (ISO 8601).
    pub updated_at: String,
}

/// An immutable snapshot of a harness board at one version.
///
/// `BoardVersion`s are never mutated: editing a harness appends a new version
/// with a monotonically increasing `version_no` rather than rewriting an
/// existing one, so any prior version remains retrievable as originally stored.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardVersion {
    /// Unique version identifier.
    pub id: String,
    /// Id of the owning [`Harness`].
    pub harness_id: String,
    /// Per-harness monotonically increasing version number.
    pub version_no: i64,
    /// The immutable board definition captured by this version.
    pub definition: BoardDefinition,
    /// Content hash deterministically derived from `definition`.
    pub content_hash: String,
    /// Creation timestamp (ISO 8601).
    pub created_at: String,
}
