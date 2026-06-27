//! Output port for harness persistence.
//!
//! Defines the [`HarnessRepository`] trait that the core depends on to store and
//! retrieve harnesses and their immutable board versions. Concrete adapters
//! (in-memory fake here, SQLite in `sugo-infra`) implement this trait and must
//! satisfy the shared contract in [`crate::contract`].

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
    }
}
