use crate::domain::board::BoardDefinition;
use crate::domain::cell::{Cell, CellStatus};
use crate::domain::harness::{BoardVersion, Harness};
use crate::error::CoreError;
use crate::ports::id_clock::IdClock;
use crate::ports::repository::HarnessRepository;
use sha2::{Digest, Sha256};

pub fn content_hash(def: &BoardDefinition) -> String {
    let json = serde_json::to_vec(def).expect("serialize board");
    let mut h = Sha256::new();
    h.update(&json);
    format!("{:x}", h.finalize())
}

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

pub struct CreateHarnessInput {
    pub name: String,
    pub definition: Option<BoardDefinition>,
}

pub struct CreateHarnessOutput {
    pub harness_id: String,
    pub version_no: i64,
    pub lock_version: i64,
}

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
    }
}
