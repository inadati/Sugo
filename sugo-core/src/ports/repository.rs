use crate::domain::harness::{BoardVersion, Harness};
use crate::error::CoreError;
use async_trait::async_trait;

#[async_trait]
pub trait HarnessRepository: Send + Sync {
    async fn create(&self, harness: &Harness, version: &BoardVersion) -> Result<(), CoreError>;
    async fn get(&self, id: &str) -> Result<Option<(Harness, BoardVersion)>, CoreError>;
    async fn list(&self) -> Result<Vec<Harness>, CoreError>;
    async fn get_version(
        &self,
        harness_id: &str,
        version_no: i64,
    ) -> Result<Option<BoardVersion>, CoreError>;
    /// head を更新しつつ新 board_version を追加。expected_lock 不一致なら LockConflict。
    async fn append_version(
        &self,
        harness: &Harness,
        version: &BoardVersion,
        expected_lock: i64,
    ) -> Result<(), CoreError>;
}

#[cfg(any(test, feature = "test-support"))]
pub mod fake {
    use super::*;
    use crate::ports::id_clock::IdClock;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU64, Ordering};

    pub struct FakeIdClock {
        counter: AtomicU64,
    }
    impl FakeIdClock {
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

    #[derive(Default)]
    pub struct InMemoryHarnessRepository {
        harnesses: Mutex<HashMap<String, Harness>>,
        versions: Mutex<HashMap<(String, i64), BoardVersion>>,
    }
    impl InMemoryHarnessRepository {
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
            // sqlite の id PRIMARY KEY と同様、重複 id は黙って上書きせず拒否する。
            if hs.contains_key(&harness.id) {
                return Err(CoreError::Storage(format!(
                    "duplicate harness id: {}",
                    harness.id
                )));
            }
            let mut vs = self.versions.lock().unwrap();
            let vkey = (version.harness_id.clone(), version.version_no);
            // sqlite の UNIQUE(harness_id, version_no) と同様、重複バージョンは拒否する。
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
            Ok(self.harnesses.lock().unwrap().values().cloned().collect())
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
            // sqlite の UNIQUE(harness_id, version_no) と同様、重複バージョンの
            // 黙った上書き（board_version 不変性の破壊）を拒否する。
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
    }
}
