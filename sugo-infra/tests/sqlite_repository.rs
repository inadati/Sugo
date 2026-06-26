use sugo_core::ports::repository::HarnessRepository;
use sugo_infra::sqlite::SqliteHarnessRepository;

mod helpers {
    use sugo_core::domain::board::BoardDefinition;
    use sugo_core::domain::cell::{Cell, CellStatus};
    use sugo_core::domain::edge::{Edge, Guard};
    use sugo_core::domain::harness::{BoardVersion, Harness};
    use sugo_core::usecase::create_harness::content_hash;

    /// 指定 prompt を持つ最小盤面を構築する。
    pub fn board(prompt: &str) -> BoardDefinition {
        BoardDefinition {
            schema_version: 1,
            start: "c1".into(),
            cells: vec![Cell {
                id: "c1".into(),
                name: "c1".into(),
                prompt: prompt.into(),
                status: CellStatus::Active,
                terminal: true,
            }],
            edges: vec![],
        }
    }

    /// guard=Some(...) を持つ edge や draft セルを含む、より複雑な盤面。
    pub fn rich_board() -> BoardDefinition {
        BoardDefinition {
            schema_version: 1,
            start: "c1".into(),
            cells: vec![
                Cell {
                    id: "c1".into(),
                    name: "intro".into(),
                    prompt: "hello".into(),
                    status: CellStatus::Active,
                    terminal: false,
                },
                Cell {
                    id: "c2".into(),
                    name: "draft-cell".into(),
                    prompt: "wip".into(),
                    status: CellStatus::Draft,
                    terminal: false,
                },
                Cell {
                    id: "c3".into(),
                    name: "done".into(),
                    prompt: "".into(),
                    status: CellStatus::Active,
                    terminal: true,
                },
            ],
            edges: vec![
                Edge {
                    from: "c1".into(),
                    to: "c2".into(),
                    label: "next".into(),
                    guard: Some(Guard { expr: "score > 0".into() }),
                },
                Edge {
                    from: "c2".into(),
                    to: "c3".into(),
                    label: "finish".into(),
                    guard: None,
                },
            ],
        }
    }

    /// 実 content_hash を計算して BoardVersion を作る。
    pub fn version(id: &str, harness_id: &str, version_no: i64, def: BoardDefinition) -> BoardVersion {
        let content_hash = content_hash(&def);
        BoardVersion {
            id: id.into(),
            harness_id: harness_id.into(),
            version_no,
            content_hash,
            definition: def,
            created_at: "t".into(),
        }
    }

    pub fn harness(id: &str, current_version: i64, lock_version: i64) -> Harness {
        Harness {
            id: id.into(),
            name: "h".into(),
            current_version,
            has_draft: false,
            lock_version,
            created_at: "t".into(),
            updated_at: "t".into(),
        }
    }

    /// h1 / v1（prompt="orig"）を作って repo に投入したものを返す。
    pub fn sample() -> (Harness, BoardVersion) {
        (harness("h1", 1, 0), version("v1", "h1", 1, board("orig")))
    }
}

use helpers::{board, harness, rich_board, version};
use sugo_core::usecase::create_harness::content_hash;

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

    let v2 = version("v2", "h1", 2, board("v2-prompt"));
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

/// (a) 不変性: v1 とは異なる内容で v2 を append しても、v1 の prompt が元のまま。
#[tokio::test]
async fn old_version_content_is_immutable_after_append() {
    let repo = SqliteHarnessRepository::in_memory().unwrap();
    let (mut h, v1) = helpers::sample(); // v1 prompt = "orig"
    repo.create(&h, &v1).await.unwrap();

    // 実際に prompt を変えた v2 を append する。
    let v2 = version("v2", "h1", 2, board("CHANGED"));
    h.current_version = 2;
    h.lock_version = 1;
    repo.append_version(&h, &v2, 0).await.unwrap();

    // append 後も v1 の definition セル prompt は元の値("orig")のまま。
    let stored_v1 = repo.get_version("h1", 1).await.unwrap().unwrap();
    assert_eq!(stored_v1.definition.cells[0].prompt, "orig");
    // v2 側は新しい値。
    let stored_v2 = repo.get_version("h1", 2).await.unwrap().unwrap();
    assert_eq!(stored_v2.definition.cells[0].prompt, "CHANGED");
}

/// (b) version_no がハーネスごとに 1,2,3 と単調増加する。
#[tokio::test]
async fn version_no_increases_monotonically() {
    let repo = SqliteHarnessRepository::in_memory().unwrap();
    let (mut h, v1) = helpers::sample();
    repo.create(&h, &v1).await.unwrap();
    assert_eq!(repo.get_version("h1", 1).await.unwrap().unwrap().version_no, 1);

    let v2 = version("v2", "h1", 2, board("p2"));
    h.current_version = 2;
    h.lock_version = 1;
    repo.append_version(&h, &v2, 0).await.unwrap();
    assert_eq!(repo.get_version("h1", 2).await.unwrap().unwrap().version_no, 2);

    let v3 = version("v3", "h1", 3, board("p3"));
    h.current_version = 3;
    h.lock_version = 2;
    repo.append_version(&h, &v3, 1).await.unwrap();
    assert_eq!(repo.get_version("h1", 3).await.unwrap().unwrap().version_no, 3);
}

/// (c) UNIQUE(harness_id, version_no): 既存 version_no と重複する append は Storage エラー。
#[tokio::test]
async fn duplicate_version_no_is_rejected() {
    let repo = SqliteHarnessRepository::in_memory().unwrap();
    let (mut h, v1) = helpers::sample();
    repo.create(&h, &v1).await.unwrap();

    // version_no=1 を再 append（id は別、lock は一致）→ UNIQUE 違反。
    let dup = version("v1b", "h1", 1, board("overwrite"));
    h.lock_version = 1;
    let err = repo.append_version(&h, &dup, 0).await.unwrap_err();
    assert!(matches!(err, sugo_core::error::CoreError::Storage(_)));

    // v1 は不変。
    let stored_v1 = repo.get_version("h1", 1).await.unwrap().unwrap();
    assert_eq!(stored_v1.definition.cells[0].prompt, "orig");
}

/// (d) lock_version 永続化: append 成功後に get で lock_version=1 が読み戻せる。
#[tokio::test]
async fn lock_version_is_persisted_after_append() {
    let repo = SqliteHarnessRepository::in_memory().unwrap();
    let (mut h, v1) = helpers::sample();
    repo.create(&h, &v1).await.unwrap();

    let v2 = version("v2", "h1", 2, board("p2"));
    h.current_version = 2;
    h.lock_version = 1;
    repo.append_version(&h, &v2, 0).await.unwrap();

    let (gh, _) = repo.get("h1").await.unwrap().unwrap();
    assert_eq!(gh.lock_version, 1);
    assert_eq!(gh.current_version, 2);
}

/// (e) stale lock 回帰: 一度成功した expected_lock_version を再利用した2回目は LockConflict。
#[tokio::test]
async fn reusing_stale_lock_version_conflicts() {
    let repo = SqliteHarnessRepository::in_memory().unwrap();
    let (mut h, v1) = helpers::sample();
    repo.create(&h, &v1).await.unwrap();

    // 1回目: expected_lock=0 で成功（lock_version は 1 になる）。
    let v2 = version("v2", "h1", 2, board("p2"));
    h.current_version = 2;
    h.lock_version = 1;
    repo.append_version(&h, &v2, 0).await.unwrap();

    // 2回目: 古い expected_lock=0 を再利用（version_no も新規にする）→ 衝突。
    let v3 = version("v3", "h1", 3, board("p3"));
    h.current_version = 3;
    h.lock_version = 2;
    let err = repo.append_version(&h, &v3, 0).await.unwrap_err();
    assert!(matches!(
        err,
        sugo_core::error::CoreError::LockConflict { .. }
    ));

    // 衝突したので v3 は永続化されておらず、head は v2 のまま。
    assert!(repo.get_version("h1", 3).await.unwrap().is_none());
    let (gh, _) = repo.get("h1").await.unwrap().unwrap();
    assert_eq!(gh.current_version, 2);
    assert_eq!(gh.lock_version, 1);
}

/// (f) 存在しない id の get は None。
#[tokio::test]
async fn get_missing_returns_none() {
    let repo = SqliteHarnessRepository::in_memory().unwrap();
    assert!(repo.get("nope").await.unwrap().is_none());
}

/// (g) content_hash ラウンドトリップ（最小ケース）: 取得後に再計算した hash が保存値と一致。
#[tokio::test]
async fn content_hash_roundtrips_minimal() {
    let repo = SqliteHarnessRepository::in_memory().unwrap();
    let (h, v) = helpers::sample();
    let expected = v.content_hash.clone();
    repo.create(&h, &v).await.unwrap();

    let stored = repo.get_version("h1", 1).await.unwrap().unwrap();
    // 保存値が偽ハッシュでなく実ハッシュ。
    assert_eq!(stored.content_hash, expected);
    // 取得した definition から再計算した hash が保存値に一致。
    assert_eq!(content_hash(&stored.definition), stored.content_hash);
}

/// (g) content_hash ラウンドトリップ（guard=Some / draft セルを含む edge ケース）。
#[tokio::test]
async fn content_hash_roundtrips_rich_definition() {
    let repo = SqliteHarnessRepository::in_memory().unwrap();
    let def = rich_board();
    let h = harness("h2", 1, 0);
    let v = version("rv1", "h2", 1, def);
    let expected = v.content_hash.clone();
    repo.create(&h, &v).await.unwrap();

    let stored = repo.get_version("h2", 1).await.unwrap().unwrap();
    assert_eq!(stored.content_hash, expected);
    // guard / draft を含む definition でも再計算した hash が一致。
    assert_eq!(content_hash(&stored.definition), stored.content_hash);
    // guard と draft が definition のラウンドトリップで失われていない。
    assert!(stored.definition.edges[0].guard.is_some());
    assert_eq!(
        stored.definition.cells[1].status,
        sugo_core::domain::cell::CellStatus::Draft
    );
}

// --- contract-test-parity: sugo-core の共有契約関数を SqliteHarnessRepository に対して実行 ---
// fake（InMemoryHarnessRepository）と sqlite が同一 ports 契約を通ることを機構的に保証する。

mod contract {
    use super::SqliteHarnessRepository;
    use sugo_core::contract;
    use sugo_core::ports::repository::fake::FakeIdClock;

    fn repo() -> SqliteHarnessRepository {
        SqliteHarnessRepository::in_memory().unwrap()
    }

    #[tokio::test]
    async fn sqlite_passes_create_get() {
        contract::contract_create_get(&repo(), &FakeIdClock::new()).await;
    }

    #[tokio::test]
    async fn sqlite_passes_get_missing_returns_none() {
        contract::contract_get_missing_returns_none(&repo()).await;
    }

    #[tokio::test]
    async fn sqlite_passes_append_creates_immutable_version() {
        contract::contract_append_creates_immutable_version(&repo()).await;
    }

    #[tokio::test]
    async fn sqlite_passes_lock_conflict() {
        contract::contract_lock_conflict(&repo()).await;
    }

    #[tokio::test]
    async fn sqlite_passes_append_to_missing_harness_is_not_found() {
        contract::contract_append_to_missing_harness_is_not_found(&repo()).await;
    }

    #[tokio::test]
    async fn sqlite_passes_duplicate_id_rejected() {
        contract::contract_duplicate_id_rejected(&repo()).await;
    }

    #[tokio::test]
    async fn sqlite_passes_duplicate_version_no_rejected() {
        contract::contract_duplicate_version_no_rejected(&repo()).await;
    }

    #[tokio::test]
    async fn sqlite_passes_list_returns_created() {
        contract::contract_list_returns_created(&repo()).await;
    }
}
