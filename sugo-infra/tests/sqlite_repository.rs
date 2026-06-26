use sugo_core::ports::repository::HarnessRepository;
use sugo_infra::sqlite::SqliteHarnessRepository;

mod helpers {
    use sugo_core::domain::board::BoardDefinition;
    use sugo_core::domain::cell::{Cell, CellStatus};
    use sugo_core::domain::harness::{BoardVersion, Harness};

    pub fn sample() -> (Harness, BoardVersion) {
        let def = BoardDefinition {
            schema_version: 1,
            start: "c1".into(),
            cells: vec![Cell {
                id: "c1".into(),
                name: "c1".into(),
                prompt: "".into(),
                status: CellStatus::Active,
                terminal: true,
            }],
            edges: vec![],
        };
        let h = Harness {
            id: "h1".into(),
            name: "h".into(),
            current_version: 1,
            has_draft: false,
            lock_version: 0,
            created_at: "t".into(),
            updated_at: "t".into(),
        };
        let v = BoardVersion {
            id: "v1".into(),
            harness_id: "h1".into(),
            version_no: 1,
            content_hash: "hash".into(),
            definition: def,
            created_at: "t".into(),
        };
        (h, v)
    }
}

#[tokio::test]
async fn create_and_get_roundtrip() {
    let repo = SqliteHarnessRepository::in_memory().unwrap();
    let (h, v) = helpers::sample();
    repo.create(&h, &v).await.unwrap();
    let (gh, gv) = repo.get("h1").await.unwrap().unwrap();
    assert_eq!(gh.name, "h");
    assert_eq!(gv.version_no, 1);
    assert_eq!(gv.definition.cells[0].id, "c1");
}

#[tokio::test]
async fn append_version_enforces_lock() {
    let repo = SqliteHarnessRepository::in_memory().unwrap();
    let (mut h, v) = helpers::sample();
    repo.create(&h, &v).await.unwrap();

    let mut v2 = v.clone();
    v2.id = "v2".into();
    v2.version_no = 2;
    h.current_version = 2;
    h.lock_version = 1;

    // 誤った expected_lock では衝突
    let err = repo.append_version(&h, &v2, 99).await.unwrap_err();
    assert!(matches!(
        err,
        sugo_core::error::CoreError::LockConflict { .. }
    ));

    // 正しい expected_lock では成功
    repo.append_version(&h, &v2, 0).await.unwrap();
    let stored = repo.get_version("h1", 2).await.unwrap().unwrap();
    assert_eq!(stored.version_no, 2);
}

#[tokio::test]
async fn board_versions_are_immutable() {
    let repo = SqliteHarnessRepository::in_memory().unwrap();
    let (mut h, v) = helpers::sample();
    repo.create(&h, &v).await.unwrap();
    let mut v2 = v.clone();
    v2.id = "v2".into();
    v2.version_no = 2;
    h.current_version = 2;
    h.lock_version = 1;
    repo.append_version(&h, &v2, 0).await.unwrap();
    // v1 は元のまま取得できる
    let v1 = repo.get_version("h1", 1).await.unwrap().unwrap();
    assert_eq!(v1.id, "v1");
}
