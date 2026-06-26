// 共有契約テスト。fake（InMemoryHarnessRepository）と sqlite
// （SqliteHarnessRepository）の両実装が同一の `HarnessRepository` ports 契約を
// 満たすことを、同一のアサーション本体で検証するためのモジュール。
//
// 各 `contract_*` 関数は `HarnessRepository` を受け取り、get/create/list/
// get_version/append_version の意味論（NotFound / LockConflict / 不変性 /
// 重複拒否）を assert する。core 側は InMemoryHarnessRepository に対して、
// sugo-infra 側は SqliteHarnessRepository に対して同じ関数を実行する。
//
// 外部クレートから呼べるよう pub かつ `test-support` feature でゲートしている。

use crate::domain::board::BoardDefinition;
use crate::domain::cell::{Cell, CellStatus};
use crate::domain::edge::Edge;
use crate::domain::harness::{BoardVersion, Harness};
use crate::error::CoreError;
use crate::ports::id_clock::IdClock;
use crate::ports::repository::HarnessRepository;

fn sample_board(prompt: &str) -> BoardDefinition {
    BoardDefinition {
        schema_version: 1,
        start: "c1".into(),
        cells: vec![
            Cell {
                id: "c1".into(),
                name: "c1".into(),
                prompt: prompt.into(),
                status: CellStatus::Active,
                terminal: false,
            },
            Cell {
                id: "c2".into(),
                name: "c2".into(),
                prompt: "".into(),
                status: CellStatus::Active,
                terminal: true,
            },
        ],
        edges: vec![Edge {
            from: "c1".into(),
            to: "c2".into(),
            label: "ok".into(),
            guard: None,
        }],
    }
}

fn harness(id: &str, name: &str, current_version: i64, lock_version: i64) -> Harness {
    Harness {
        id: id.into(),
        name: name.into(),
        current_version,
        has_draft: false,
        lock_version,
        created_at: "2026-01-01T00:00:00+09:00".into(),
        updated_at: "2026-01-01T00:00:00+09:00".into(),
    }
}

fn version(id: &str, harness_id: &str, version_no: i64, def: BoardDefinition) -> BoardVersion {
    BoardVersion {
        id: id.into(),
        harness_id: harness_id.into(),
        version_no,
        content_hash: format!("hash-{version_no}"),
        definition: def,
        created_at: "2026-01-01T00:00:00+09:00".into(),
    }
}

/// create → get で同一内容が取得できること。
pub async fn contract_create_get<R: HarnessRepository>(repo: &R, _clock: &dyn IdClock) {
    let h = harness("h1", "first", 1, 0);
    let v = version("v1", "h1", 1, sample_board("p1"));
    repo.create(&h, &v).await.expect("create ok");

    let (got_h, got_v) = repo.get("h1").await.expect("get ok").expect("present");
    assert_eq!(got_h.id, "h1");
    assert_eq!(got_h.name, "first");
    assert_eq!(got_h.current_version, 1);
    assert_eq!(got_v.version_no, 1);
    assert_eq!(got_v.definition.cells[0].prompt, "p1");
}

/// 存在しない id の get は None を返すこと。
pub async fn contract_get_missing_returns_none<R: HarnessRepository>(repo: &R) {
    assert!(repo.get("nope").await.expect("get ok").is_none());
}

/// append_version が旧バージョンを書き換えず新バージョンを追加し、
/// 旧バージョンの内容が不変であること。
pub async fn contract_append_creates_immutable_version<R: HarnessRepository>(repo: &R) {
    let h = harness("h1", "first", 1, 0);
    let v1 = version("v1", "h1", 1, sample_board("original"));
    repo.create(&h, &v1).await.expect("create ok");

    let mut h2 = h.clone();
    h2.current_version = 2;
    h2.lock_version = 1;
    let v2 = version("v2", "h1", 2, sample_board("changed"));
    repo.append_version(&h2, &v2, 0).await.expect("append ok");

    // 旧バージョン v1 の内容が元のまま。
    let got_v1 = repo
        .get_version("h1", 1)
        .await
        .expect("get_version ok")
        .expect("v1 present");
    assert_eq!(got_v1.definition.cells[0].prompt, "original");

    // head が v2 へ更新され lock_version も永続化されている。
    let (got_h, got_v2) = repo.get("h1").await.expect("get ok").expect("present");
    assert_eq!(got_h.current_version, 2);
    assert_eq!(got_h.lock_version, 1);
    assert_eq!(got_v2.definition.cells[0].prompt, "changed");
}

/// expected_lock 不一致で append すると LockConflict になること。
pub async fn contract_lock_conflict<R: HarnessRepository>(repo: &R) {
    let h = harness("h1", "first", 1, 0);
    let v1 = version("v1", "h1", 1, sample_board("p1"));
    repo.create(&h, &v1).await.expect("create ok");

    let mut h2 = h.clone();
    h2.current_version = 2;
    h2.lock_version = 1;
    let v2 = version("v2", "h1", 2, sample_board("p2"));
    let err = repo
        .append_version(&h2, &v2, 99)
        .await
        .expect_err("should conflict");
    assert!(matches!(err, CoreError::LockConflict { .. }));
}

/// 存在しない harness への append は NotFound になること。
pub async fn contract_append_to_missing_harness_is_not_found<R: HarnessRepository>(repo: &R) {
    let h = harness("ghost", "x", 2, 1);
    let v = version("v2", "ghost", 2, sample_board("p"));
    let err = repo
        .append_version(&h, &v, 0)
        .await
        .expect_err("should be not found");
    assert!(matches!(err, CoreError::NotFound(_)));
}

/// 重複 id の create は黙って上書きせず Err になること。
pub async fn contract_duplicate_id_rejected<R: HarnessRepository>(repo: &R) {
    let h = harness("h1", "first", 1, 0);
    let v = version("v1", "h1", 1, sample_board("p1"));
    repo.create(&h, &v).await.expect("create ok");

    let dup_h = harness("h1", "second", 1, 0);
    let dup_v = version("v9", "h1", 1, sample_board("other"));
    assert!(repo.create(&dup_h, &dup_v).await.is_err());

    // 元の内容が保持されている（黙った上書きが起きていない）。
    let (got_h, _) = repo.get("h1").await.expect("get ok").expect("present");
    assert_eq!(got_h.name, "first");
}

/// 重複 version_no の append は黙って上書きせず Err になること（不変性強制）。
pub async fn contract_duplicate_version_no_rejected<R: HarnessRepository>(repo: &R) {
    let h = harness("h1", "first", 1, 0);
    let v1 = version("v1", "h1", 1, sample_board("original"));
    repo.create(&h, &v1).await.expect("create ok");

    // version_no=1 を再 append（lock は一致させる）→ UNIQUE 違反相当。
    let mut h2 = h.clone();
    h2.lock_version = 1;
    let dup_v = version("v1b", "h1", 1, sample_board("overwrite"));
    assert!(repo.append_version(&h2, &dup_v, 0).await.is_err());

    // v1 の内容が不変。
    let got_v1 = repo
        .get_version("h1", 1)
        .await
        .expect("get_version ok")
        .expect("v1 present");
    assert_eq!(got_v1.definition.cells[0].prompt, "original");
}

/// list が登録済みハーネスを返すこと。
pub async fn contract_list_returns_created<R: HarnessRepository>(repo: &R) {
    let h = harness("h1", "first", 1, 0);
    let v = version("v1", "h1", 1, sample_board("p1"));
    repo.create(&h, &v).await.expect("create ok");

    let listed = repo.list().await.expect("list ok");
    assert!(listed.iter().any(|x| x.id == "h1"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::repository::fake::{FakeIdClock, InMemoryHarnessRepository};

    #[tokio::test]
    async fn fake_passes_create_get() {
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();
        contract_create_get(&repo, &clock).await;
    }

    #[tokio::test]
    async fn fake_passes_get_missing_returns_none() {
        let repo = InMemoryHarnessRepository::new();
        contract_get_missing_returns_none(&repo).await;
    }

    #[tokio::test]
    async fn fake_passes_append_creates_immutable_version() {
        let repo = InMemoryHarnessRepository::new();
        contract_append_creates_immutable_version(&repo).await;
    }

    #[tokio::test]
    async fn fake_passes_lock_conflict() {
        let repo = InMemoryHarnessRepository::new();
        contract_lock_conflict(&repo).await;
    }

    #[tokio::test]
    async fn fake_passes_append_to_missing_harness_is_not_found() {
        let repo = InMemoryHarnessRepository::new();
        contract_append_to_missing_harness_is_not_found(&repo).await;
    }

    #[tokio::test]
    async fn fake_passes_duplicate_id_rejected() {
        let repo = InMemoryHarnessRepository::new();
        contract_duplicate_id_rejected(&repo).await;
    }

    #[tokio::test]
    async fn fake_passes_duplicate_version_no_rejected() {
        let repo = InMemoryHarnessRepository::new();
        contract_duplicate_version_no_rejected(&repo).await;
    }

    #[tokio::test]
    async fn fake_passes_list_returns_created() {
        let repo = InMemoryHarnessRepository::new();
        contract_list_returns_created(&repo).await;
    }
}
