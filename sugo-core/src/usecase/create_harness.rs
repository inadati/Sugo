//! Use case for creating a new harness from a board definition.
//!
//! Validates and persists the initial board version (version 1) and exposes
//! [`content_hash`], the canonical board-definition hashing used to stamp every
//! [`BoardVersion`] for integrity and deduplication.

use crate::domain::board::BoardDefinition;
use crate::domain::cell::{Cell, CellStatus};
use crate::domain::harness::{BoardVersion, Harness};
use crate::error::CoreError;
use crate::ports::id_clock::IdClock;
use crate::ports::repository::HarnessRepository;
use sha2::{Digest, Sha256};

/// Computes the SHA-256 content hash of a board definition.
///
/// The hash is taken over the struct-field-declaration-order JSON produced by
/// `serde_json::to_vec` and stored on each [`BoardVersion`] so identical
/// definitions (with the same field values in the same field order) produce
/// identical hashes, supporting integrity checks and deduplication.
pub fn content_hash(def: &BoardDefinition) -> String {
    let json = serde_json::to_vec(def).expect("serialize board");
    let mut h = Sha256::new();
    h.update(&json);
    format!("{:x}", h.finalize())
}

/// Builds the minimal default board used when no definition is supplied.
///
/// A single active, terminal `start` cell with an empty prompt — a valid board
/// the agent can immediately edit and extend.
pub fn default_board() -> BoardDefinition {
    BoardDefinition {
        schema_version: 1,
        start: "start".into(),
        cells: vec![Cell {
            id: "start".into(),
            name: "start".into(),
            prompt: String::new(),
            status: CellStatus::Active,
            terminal: true,
        }],
        edges: vec![],
    }
}

/// Input for [`create_harness`].
pub struct CreateHarnessInput {
    /// Name to assign the new harness.
    pub name: String,
    /// Optional initial board; when `None`, [`default_board`] is used.
    pub definition: Option<BoardDefinition>,
}

/// Output of [`create_harness`].
pub struct CreateHarnessOutput {
    /// Id of the newly created harness.
    pub harness_id: String,
    /// `version_no` of the initial board version (always 1).
    pub version_no: i64,
    /// Initial optimistic-lock version (always 0).
    pub lock_version: i64,
}

/// Creates a harness with its initial board version (v1).
///
/// Uses the provided definition or [`default_board`] when omitted, derives
/// `has_draft` from the definition's cells, and persists the harness head plus
/// its first immutable [`BoardVersion`] in a single repository write.
pub async fn create_harness(
    repo: &dyn HarnessRepository,
    clock: &dyn IdClock,
    input: CreateHarnessInput,
) -> Result<CreateHarnessOutput, CoreError> {
    let def = input.definition.unwrap_or_else(default_board);
    let has_draft = def.cells.iter().any(|c| c.status == CellStatus::Draft);
    let now = clock.now_iso();
    let harness = Harness {
        id: clock.new_id(),
        name: input.name,
        current_version: 1,
        has_draft,
        lock_version: 0,
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    let version = BoardVersion {
        id: clock.new_id(),
        harness_id: harness.id.clone(),
        version_no: 1,
        content_hash: content_hash(&def),
        definition: def,
        created_at: now,
    };
    repo.create(&harness, &version).await?;
    Ok(CreateHarnessOutput { harness_id: harness.id, version_no: 1, lock_version: 0 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::cell::{Cell, CellStatus};
    use crate::ports::repository::fake::{FakeIdClock, InMemoryHarnessRepository};

    #[tokio::test]
    async fn creates_harness_with_v1() {
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();
        let out = create_harness(
            &repo,
            &clock,
            CreateHarnessInput { name: "h".into(), definition: None },
        )
        .await
        .unwrap();
        assert_eq!(out.version_no, 1);
        assert_eq!(out.lock_version, 0);
        let (h, v) = repo.get(&out.harness_id).await.unwrap().unwrap();
        assert_eq!(h.current_version, 1);
        assert_eq!(v.version_no, 1);
        assert_eq!(h.name, "h");
        assert!(!v.content_hash.is_empty(), "content_hash must be non-empty");
        // SHA-256 hexadecimal is exactly 64 characters
        assert_eq!(v.content_hash.len(), 64, "content_hash must be SHA-256 hex (64 chars)");
    }

    #[tokio::test]
    async fn create_with_supplied_definition_persists_that_definition() {
        // The Some(definition) path: the harness head must carry the supplied
        // board verbatim rather than the default board.
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();
        let def = BoardDefinition {
            schema_version: 1,
            start: "c1".into(),
            cells: vec![Cell {
                id: "c1".into(),
                name: "intro".into(),
                prompt: "hello".into(),
                status: CellStatus::Active,
                terminal: true,
            }],
            edges: vec![],
        };
        let out = create_harness(
            &repo,
            &clock,
            CreateHarnessInput { name: "h".into(), definition: Some(def.clone()) },
        )
        .await
        .unwrap();
        let (h, v) = repo.get(&out.harness_id).await.unwrap().unwrap();
        assert!(!h.has_draft);
        assert_eq!(v.definition, def);
        assert_eq!(v.definition.cells[0].id, "c1");
        assert_eq!(v.definition.cells[0].prompt, "hello");
    }

    #[tokio::test]
    async fn create_with_draft_cell_sets_has_draft() {
        // A definition containing a Draft cell must mark the harness has_draft.
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();
        let def = BoardDefinition {
            schema_version: 1,
            start: "c1".into(),
            cells: vec![
                Cell {
                    id: "c1".into(),
                    name: "c1".into(),
                    prompt: "p".into(),
                    status: CellStatus::Active,
                    terminal: true,
                },
                Cell {
                    id: "c2".into(),
                    name: "draftcell".into(),
                    prompt: "".into(),
                    status: CellStatus::Draft,
                    terminal: false,
                },
            ],
            edges: vec![],
        };
        let out = create_harness(
            &repo,
            &clock,
            CreateHarnessInput { name: "h".into(), definition: Some(def) },
        )
        .await
        .unwrap();
        let (h, _v) = repo.get(&out.harness_id).await.unwrap().unwrap();
        assert!(h.has_draft);
    }

    #[test]
    fn content_hash_is_deterministic() {
        // Two independently constructed BoardDefinition values with identical
        // fields must produce the same hash: the hash depends only on content,
        // not on the specific memory address or allocation order.
        let make_def = || BoardDefinition {
            schema_version: 1,
            start: "c1".into(),
            cells: vec![Cell {
                id: "c1".into(),
                name: "intro".into(),
                prompt: "hello".into(),
                status: CellStatus::Active,
                terminal: true,
            }],
            edges: vec![],
        };
        assert_eq!(content_hash(&make_def()), content_hash(&make_def()));
    }
}
