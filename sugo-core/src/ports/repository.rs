//! Output port for harness persistence.
//!
//! Defines the [`HarnessRepository`] trait that the core depends on to store and
//! retrieve harnesses and their immutable board versions. Concrete adapters
//! (in-memory fake here, SQLite in `sugo-infra`) implement this trait and must
//! satisfy the shared contract in [`crate::contract`].

use crate::domain::folder::Folder;
use crate::domain::harness::{BoardVersion, Harness};
use crate::error::CoreError;
use async_trait::async_trait;

/// Persistence contract for harnesses and their immutable board versions.
///
/// Implementations must enforce that board versions are append-only (existing
/// versions are never overwritten) and that head updates use optimistic
/// locking. The same behaviour is exercised against every implementation via
/// the shared contract tests in [`crate::contract`].
#[async_trait]
pub trait HarnessRepository: Send + Sync {
    /// Insert a new harness together with its initial board version.
    ///
    /// Returns `Err(CoreError::Storage)` if a harness with the same id, or a
    /// `(harness_id, version_no)` pair, already exists (no silent overwrite).
    async fn create(&self, harness: &Harness, version: &BoardVersion) -> Result<(), CoreError>;
    /// Fetch a harness and its current head board version.
    ///
    /// Returns `Ok(None)` when no harness with `id` exists. Only the head
    /// version is returned; use [`HarnessRepository::get_version`] for older
    /// versions.
    async fn get(&self, id: &str) -> Result<Option<(Harness, BoardVersion)>, CoreError>;
    /// List all stored harnesses (head metadata only, in unspecified order).
    async fn list(&self) -> Result<Vec<Harness>, CoreError>;
    /// Fetch a specific historical board version by `version_no`.
    ///
    /// Returns `Ok(None)` when no such version exists for the harness.
    async fn get_version(
        &self,
        harness_id: &str,
        version_no: i64,
    ) -> Result<Option<BoardVersion>, CoreError>;
    /// Append a new immutable board version and move the harness head to it.
    ///
    /// Uses optimistic locking: `expected_lock` must equal the harness's
    /// current `lock_version`, otherwise `Err(CoreError::LockConflict)` is
    /// returned and nothing is written. Returns `Err(CoreError::NotFound)` if
    /// the harness does not exist, and `Err(CoreError::Storage)` if the new
    /// `version_no` would overwrite an existing version.
    async fn append_version(
        &self,
        harness: &Harness,
        version: &BoardVersion,
        expected_lock: i64,
    ) -> Result<(), CoreError>;

    /// ハーネスをゴミ箱に移動する（`deleted_at` をセット）。
    /// `id` が存在しない場合は `CoreError::NotFound`。
    async fn trash_harness(&self, id: &str, deleted_at: &str) -> Result<(), CoreError>;
    /// ゴミ箱からハーネスを復活させる（`deleted_at` をクリア）。
    /// `id` がゴミ箱に存在しない場合は `CoreError::NotFound`。
    async fn restore_harness(&self, id: &str) -> Result<(), CoreError>;
    /// ハーネスとその全 board_versions を物理削除する。
    /// `id` が存在しない場合は `CoreError::NotFound`。
    async fn purge_harness(&self, id: &str) -> Result<(), CoreError>;
    /// ゴミ箱の一覧を返す。各要素は `(harness_id, name, deleted_at)` のタプル。
    async fn list_trash(&self) -> Result<Vec<(String, String, String)>, CoreError>;
    /// `before_iso` より前にゴミ箱に入ったハーネスを自動物理削除する。
    async fn purge_trash_before(&self, before_iso: &str) -> Result<(), CoreError>;
    /// ハーネスの説明文を更新する（description カラムのみ更新、ボードバージョンは変更しない）。
    async fn set_description(&self, id: &str, description: Option<&str>) -> Result<(), CoreError>;

    /// フォルダ一覧を返す。各要素は `(フォルダ, 所属する未削除ハーネスの件数)`。
    /// `sort_order` の昇順で返すこと。
    async fn list_folders(&self) -> Result<Vec<(Folder, i64)>, CoreError>;
    /// フォルダを1件追加する。同一 id が既に存在する場合は `CoreError::Storage`。
    /// 名前の重複判定は usecase 層の責務であり、ここでは行わない。
    async fn create_folder(&self, folder: &Folder) -> Result<(), CoreError>;
    /// フォルダ名を更新する。`id` が存在しない場合は `CoreError::NotFound`。
    async fn rename_folder(
        &self,
        id: &str,
        name: &str,
        updated_at: &str,
    ) -> Result<(), CoreError>;
    /// フォルダを削除する。所属ハーネスの `folder_id` を NULL に戻してから
    /// フォルダ行を削除する（単一トランザクション）。ハーネスは削除しない。
    /// `id` が存在しない場合は `CoreError::NotFound`。
    async fn delete_folder(&self, id: &str) -> Result<(), CoreError>;
    /// ハーネスの所属フォルダを変更する。`folder_id` が `None` なら未分類へ。
    /// ハーネスまたはフォルダが存在しない場合は `CoreError::NotFound`。
    async fn move_harness_to_folder(
        &self,
        harness_id: &str,
        folder_id: Option<&str>,
    ) -> Result<(), CoreError>;
}

#[cfg(any(test, feature = "test-support"))]
pub mod fake {
    //! Deterministic in-memory fakes for the persistence and id/clock ports.
    //!
    //! Gated behind `cfg(any(test, feature = "test-support"))`, this module
    //! provides [`FakeIdClock`] and [`InMemoryHarnessRepository`] -- in-memory
    //! implementations used by core unit tests and, via the `test-support`
    //! feature, re-exported so `sugo-infra`'s cross-crate tests can exercise the
    //! shared contract against the same reference behaviour (they name these
    //! types directly).
    use super::*;
    use crate::ports::id_clock::IdClock;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Deterministic [`IdClock`] for tests: ids count up (`id-0`, `id-1`, ...)
    /// and the clock returns a fixed timestamp.
    ///
    /// Named directly by `sugo-infra`'s tests via the `test-support` feature.
    pub struct FakeIdClock {
        counter: AtomicU64,
    }
    impl FakeIdClock {
        /// Create a `FakeIdClock` whose id counter starts at zero.
        pub fn new() -> Self {
            Self { counter: AtomicU64::new(0) }
        }
    }
    impl Default for FakeIdClock {
        fn default() -> Self {
            Self::new()
        }
    }
    impl IdClock for FakeIdClock {
        fn new_id(&self) -> String {
            let n = self.counter.fetch_add(1, Ordering::SeqCst);
            format!("id-{n}")
        }
        fn now_iso(&self) -> String {
            "2026-01-01T00:00:00+09:00".into()
        }
    }

    /// In-memory [`HarnessRepository`] backing the shared contract tests.
    ///
    /// Stores harnesses and their board versions in `Mutex`-guarded maps,
    /// mirroring the sqlite adapter's invariants (duplicate-id/version rejection,
    /// optimistic locking, version immutability) so both implementations can be
    /// driven by the same contract assertions.
    #[derive(Default)]
    pub struct InMemoryHarnessRepository {
        harnesses: Mutex<HashMap<String, Harness>>,
        versions: Mutex<HashMap<(String, i64), BoardVersion>>,
        deleted_at: Mutex<HashMap<String, String>>,
        folders: Mutex<HashMap<String, Folder>>,
    }
    impl InMemoryHarnessRepository {
        /// Create an empty in-memory repository.
        pub fn new() -> Self {
            Self::default()
        }
    }

    #[async_trait]
    impl HarnessRepository for InMemoryHarnessRepository {
        async fn create(
            &self,
            harness: &Harness,
            version: &BoardVersion,
        ) -> Result<(), CoreError> {
            let mut hs = self.harnesses.lock().unwrap();
            // Like sqlite's id PRIMARY KEY, reject a duplicate id instead of
            // silently overwriting.
            if hs.contains_key(&harness.id) {
                return Err(CoreError::Storage(format!(
                    "duplicate harness id: {}",
                    harness.id
                )));
            }
            let mut vs = self.versions.lock().unwrap();
            let vkey = (version.harness_id.clone(), version.version_no);
            // Like sqlite's UNIQUE(harness_id, version_no), reject a duplicate
            // version.
            if vs.contains_key(&vkey) {
                return Err(CoreError::Storage(format!(
                    "duplicate version_no {} for harness {}",
                    version.version_no, version.harness_id
                )));
            }
            hs.insert(harness.id.clone(), harness.clone());
            vs.insert(vkey, version.clone());
            Ok(())
        }
        async fn get(&self, id: &str) -> Result<Option<(Harness, BoardVersion)>, CoreError> {
            let h = self.harnesses.lock().unwrap().get(id).cloned();
            match h {
                None => Ok(None),
                Some(h) => {
                    let v = self
                        .versions
                        .lock()
                        .unwrap()
                        .get(&(id.to_string(), h.current_version))
                        .cloned()
                        .ok_or_else(|| CoreError::Storage("missing head version".into()))?;
                    Ok(Some((h, v)))
                }
            }
        }
        async fn list(&self) -> Result<Vec<Harness>, CoreError> {
            let hs = self.harnesses.lock().unwrap();
            let deleted = self.deleted_at.lock().unwrap();
            Ok(hs
                .values()
                .filter(|h| !deleted.contains_key(&h.id))
                .cloned()
                .collect())
        }
        async fn get_version(
            &self,
            harness_id: &str,
            version_no: i64,
        ) -> Result<Option<BoardVersion>, CoreError> {
            Ok(self
                .versions
                .lock()
                .unwrap()
                .get(&(harness_id.to_string(), version_no))
                .cloned())
        }
        async fn append_version(
            &self,
            harness: &Harness,
            version: &BoardVersion,
            expected_lock: i64,
        ) -> Result<(), CoreError> {
            let mut hs = self.harnesses.lock().unwrap();
            let cur = hs
                .get(&harness.id)
                .ok_or_else(|| CoreError::NotFound(harness.id.clone()))?;
            if cur.lock_version != expected_lock {
                return Err(CoreError::LockConflict {
                    expected: expected_lock,
                    actual: cur.lock_version,
                });
            }
            let mut vs = self.versions.lock().unwrap();
            let vkey = (version.harness_id.clone(), version.version_no);
            // Like sqlite's UNIQUE(harness_id, version_no), reject a silent
            // overwrite of an existing version (which would break board_version
            // immutability).
            if vs.contains_key(&vkey) {
                return Err(CoreError::Storage(format!(
                    "duplicate version_no {} for harness {}",
                    version.version_no, version.harness_id
                )));
            }
            hs.insert(harness.id.clone(), harness.clone());
            vs.insert(vkey, version.clone());
            Ok(())
        }

        async fn trash_harness(&self, id: &str, deleted_at: &str) -> Result<(), CoreError> {
            let hs = self.harnesses.lock().unwrap();
            if !hs.contains_key(id) {
                return Err(CoreError::NotFound(id.to_string()));
            }
            drop(hs);
            self.deleted_at
                .lock()
                .unwrap()
                .insert(id.to_string(), deleted_at.to_string());
            Ok(())
        }

        async fn restore_harness(&self, id: &str) -> Result<(), CoreError> {
            let removed = self.deleted_at.lock().unwrap().remove(id);
            if removed.is_none() {
                return Err(CoreError::NotFound(id.to_string()));
            }
            Ok(())
        }

        async fn purge_harness(&self, id: &str) -> Result<(), CoreError> {
            let mut hs = self.harnesses.lock().unwrap();
            if hs.remove(id).is_none() {
                return Err(CoreError::NotFound(id.to_string()));
            }
            let mut vs = self.versions.lock().unwrap();
            vs.retain(|(hid, _), _| hid != id);
            self.deleted_at.lock().unwrap().remove(id);
            Ok(())
        }

        async fn list_trash(&self) -> Result<Vec<(String, String, String)>, CoreError> {
            let hs = self.harnesses.lock().unwrap();
            let deleted = self.deleted_at.lock().unwrap();
            Ok(deleted
                .iter()
                .filter_map(|(id, ts)| {
                    hs.get(id)
                        .map(|h| (id.clone(), h.name.clone(), ts.clone()))
                })
                .collect())
        }

        async fn purge_trash_before(&self, before_iso: &str) -> Result<(), CoreError> {
            let to_purge: Vec<String> = {
                let deleted = self.deleted_at.lock().unwrap();
                deleted
                    .iter()
                    .filter(|(_, ts)| ts.as_str() < before_iso)
                    .map(|(id, _)| id.clone())
                    .collect()
            };
            for id in to_purge {
                self.purge_harness(&id).await?;
            }
            Ok(())
        }

        async fn set_description(&self, id: &str, description: Option<&str>) -> Result<(), CoreError> {
            let mut hs = self.harnesses.lock().unwrap();
            let h = hs.get_mut(id).ok_or_else(|| CoreError::NotFound(id.to_string()))?;
            h.description = description.map(|s| s.to_string());
            Ok(())
        }

        async fn list_folders(&self) -> Result<Vec<(Folder, i64)>, CoreError> {
            let folders = self.folders.lock().unwrap();
            let hs = self.harnesses.lock().unwrap();
            let deleted = self.deleted_at.lock().unwrap();
            let mut out: Vec<(Folder, i64)> = folders
                .values()
                .map(|f| {
                    let count = hs
                        .values()
                        .filter(|h| {
                            h.folder_id.as_deref() == Some(f.id.as_str())
                                && !deleted.contains_key(&h.id)
                        })
                        .count() as i64;
                    (f.clone(), count)
                })
                .collect();
            out.sort_by(|a, b| {
                a.0.sort_order
                    .cmp(&b.0.sort_order)
                    .then_with(|| a.0.created_at.cmp(&b.0.created_at))
            });
            Ok(out)
        }

        async fn create_folder(&self, folder: &Folder) -> Result<(), CoreError> {
            let mut folders = self.folders.lock().unwrap();
            if folders.contains_key(&folder.id) {
                return Err(CoreError::Storage(format!(
                    "duplicate folder id: {}",
                    folder.id
                )));
            }
            folders.insert(folder.id.clone(), folder.clone());
            Ok(())
        }

        async fn rename_folder(
            &self,
            id: &str,
            name: &str,
            updated_at: &str,
        ) -> Result<(), CoreError> {
            let mut folders = self.folders.lock().unwrap();
            let f = folders
                .get_mut(id)
                .ok_or_else(|| CoreError::NotFound(id.to_string()))?;
            f.name = name.to_string();
            f.updated_at = updated_at.to_string();
            Ok(())
        }

        async fn delete_folder(&self, id: &str) -> Result<(), CoreError> {
            let mut folders = self.folders.lock().unwrap();
            if !folders.contains_key(id) {
                return Err(CoreError::NotFound(id.to_string()));
            }
            let mut hs = self.harnesses.lock().unwrap();
            for h in hs.values_mut() {
                if h.folder_id.as_deref() == Some(id) {
                    h.folder_id = None;
                }
            }
            folders.remove(id);
            Ok(())
        }

        async fn move_harness_to_folder(
            &self,
            harness_id: &str,
            folder_id: Option<&str>,
        ) -> Result<(), CoreError> {
            if let Some(fid) = folder_id {
                let folders = self.folders.lock().unwrap();
                if !folders.contains_key(fid) {
                    return Err(CoreError::NotFound(fid.to_string()));
                }
            }
            let mut hs = self.harnesses.lock().unwrap();
            let h = hs
                .get_mut(harness_id)
                .ok_or_else(|| CoreError::NotFound(harness_id.to_string()))?;
            h.folder_id = folder_id.map(|s| s.to_string());
            Ok(())
        }
    }

    /// テスト用の最小ハーネスとその v1 ボードバージョン。
    pub fn sample_harness(id: &str) -> (Harness, BoardVersion) {
        use crate::domain::board::BoardDefinition;
        use crate::domain::cell::{Cell, CellStatus};
        let def = BoardDefinition {
            schema_version: 1,
            start: "c1".into(),
            cells: vec![Cell {
                id: "c1".into(),
                name: "c1".into(),
                prompt: "p".into(),
                status: CellStatus::Active,
                terminal: true,
                request_memo: String::new(),
            }],
            edges: vec![],
        };
        (
            Harness {
                id: id.into(),
                name: "h".into(),
                description: None,
                folder_id: None,
                current_version: 1,
                has_draft: false,
                lock_version: 0,
                created_at: "t".into(),
                updated_at: "t".into(),
            },
            BoardVersion {
                id: format!("v-{id}"),
                harness_id: id.into(),
                version_no: 1,
                definition: def,
                content_hash: "hash".into(),
                created_at: "t".into(),
            },
        )
    }
}

#[cfg(test)]
mod fake_tests {
    use super::fake::InMemoryHarnessRepository;
    use super::*;
    use crate::domain::folder::Folder;

    fn folder(id: &str, name: &str, order: i64) -> Folder {
        Folder {
            id: id.into(),
            name: name.into(),
            parent_id: None,
            sort_order: order,
            created_at: "2026-08-26T00:00:00+09:00".into(),
            updated_at: "2026-08-26T00:00:00+09:00".into(),
        }
    }

    #[tokio::test]
    async fn create_and_list_folders() {
        let repo = InMemoryHarnessRepository::new();
        repo.create_folder(&folder("f1", "開発", 0)).await.unwrap();
        let listed = repo.list_folders().await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].0.name, "開発");
        assert_eq!(listed[0].1, 0, "空フォルダの件数は 0");
    }

    #[tokio::test]
    async fn delete_folder_moves_harnesses_to_uncategorized() {
        let repo = InMemoryHarnessRepository::new();
        repo.create_folder(&folder("f1", "開発", 0)).await.unwrap();
        // ハーネスを1件作って f1 に入れる
        let (h, v) = super::fake::sample_harness("h1");
        repo.create(&h, &v).await.unwrap();
        repo.move_harness_to_folder("h1", Some("f1")).await.unwrap();
        assert_eq!(repo.list_folders().await.unwrap()[0].1, 1);

        repo.delete_folder("f1").await.unwrap();

        assert!(repo.list_folders().await.unwrap().is_empty());
        let listed = repo.list().await.unwrap();
        assert_eq!(listed.len(), 1, "ハーネスは削除されない");
        assert!(listed[0].folder_id.is_none(), "未分類に戻る");
    }

    #[tokio::test]
    async fn delete_folder_nulls_folder_id_of_trashed_harness() {
        // Regression guard for design.md L76: deleting a folder must null out
        // folder_id even for a harness that is currently in the trash, not
        // just live ones — otherwise a later restore would resurrect a
        // harness pointing at a folder that no longer exists.
        let repo = InMemoryHarnessRepository::new();
        repo.create_folder(&folder("f1", "開発", 0)).await.unwrap();
        let (h, v) = super::fake::sample_harness("h1");
        repo.create(&h, &v).await.unwrap();
        repo.move_harness_to_folder("h1", Some("f1")).await.unwrap();
        repo.trash_harness("h1", "2026-08-26T10:00:00+09:00").await.unwrap();

        // Sanity: the trashed harness is still fetchable and still points at
        // f1 before the folder is deleted.
        let (fetched, _) = repo
            .get("h1")
            .await
            .unwrap()
            .expect("trashed harness must still be fetchable via get()");
        assert_eq!(fetched.folder_id.as_deref(), Some("f1"));

        repo.delete_folder("f1").await.unwrap();

        let (fetched, _) = repo
            .get("h1")
            .await
            .unwrap()
            .expect("trashed harness must still be fetchable after delete_folder");
        assert!(
            fetched.folder_id.is_none(),
            "trashed harness's folder_id must be nulled by delete_folder, got {:?}",
            fetched.folder_id
        );
    }

    #[tokio::test]
    async fn rename_folder_unknown_id_is_not_found() {
        let repo = InMemoryHarnessRepository::new();
        let err = repo
            .rename_folder("ghost", "新名称", "2026-08-26T00:00:00+09:00")
            .await
            .unwrap_err();
        assert!(matches!(err, CoreError::NotFound(_)));
    }

    #[tokio::test]
    async fn move_harness_unknown_id_is_not_found() {
        let repo = InMemoryHarnessRepository::new();
        let err = repo.move_harness_to_folder("ghost", None).await.unwrap_err();
        assert!(matches!(err, CoreError::NotFound(_)));
    }
}
