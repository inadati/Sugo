# サイドバー + ハーネスゴミ箱 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ハーネス一覧画面に左サイドバーを追加し、ゴミ箱（ソフトデリート）機能を実装する。

**Architecture:** ShellLayout がサイドバー（AppSidebar）と RouterView を2カラムで並べる。ゴミ箱は `harnesses.deleted_at` カラムで管理するソフトデリート。新規Tauriコマンド `trash_harness` / `restore_harness` / `purge_harness` / `list_trash` を追加し、起動時に180日超の項目を自動パージする。

**Tech Stack:** Rust/Tauri, rusqlite, chrono, Vue 3 + vue-router v4, @heroicons/vue, Tailwind CSS, Vitest

## Global Constraints

- アイコンは必ず `@heroicons/vue/24/outline` の SVG コンポーネントを使う。絵文字をアイコンに使わない。
- ゴミ箱の自動削除期限は **180日固定**。
- 既存のポーリング間隔 `2000ms` に合わせる。
- 楽観ロックの安定コード（`"lock_conflict"` 等）を壊さない。
- コミットは `feat/sidebar-trash` ブランチで行う（既に作成済み）。

---

## ファイル構成

| 操作 | ファイル |
|---|---|
| Modify | `sugo-core/src/ports/repository.rs` |
| Modify | `sugo-infra/src/sqlite/repository.rs` |
| Modify | `sugo-gui/src-tauri/src/dto.rs` |
| Modify | `sugo-gui/src-tauri/src/commands.rs` |
| Modify | `sugo-gui/src-tauri/src/lib.rs` |
| Modify | `sugo-gui/src/App.vue` |
| Modify | `sugo-gui/src/router/index.ts` |
| Modify | `sugo-gui/src/views/HarnessList.vue` |
| Modify | `sugo-gui/src/views/HarnessList.test.ts` |
| Create | `sugo-gui/src/layouts/ShellLayout.vue` |
| Create | `sugo-gui/src/components/AppSidebar.vue` |
| Create | `sugo-gui/src/views/TrashView.vue` |

---

### Task 1: Backend – DBマイグレーション (`deleted_at` 追加 + `list()` フィルタ)

**Files:**
- Modify: `sugo-infra/src/sqlite/repository.rs:46-110` (init + list)

**Interfaces:**
- Produces: `harnesses.deleted_at TEXT NULL` カラム（既存DBには idempotent に追加される）
- Produces: `list()` が `WHERE deleted_at IS NULL` のみ返す

- [ ] **Step 1: `list()` フィルタのテストを書く**

`sugo-infra/src/sqlite/repository.rs` の `#[cfg(test)] mod tests` 末尾に追加：

```rust
#[tokio::test]
async fn list_excludes_trashed_harnesses() {
    let repo = SqliteHarnessRepository::in_memory().expect("in-memory repo");
    // seed two harnesses
    repo.create(&harness("h1", 1, 0), &version("v1", "h1", 1, "p1"))
        .await
        .expect("seed h1");
    repo.create(&harness("h2", 1, 0), &version("v2", "h2", 1, "p2"))
        .await
        .expect("seed h2");
    // trash h1 directly via SQL
    let now = "2026-06-30T10:00:00+09:00";
    repo.lock()
        .execute("UPDATE harnesses SET deleted_at = ?1 WHERE id = ?2", [now, "h1"])
        .expect("set deleted_at");
    let listed = repo.list().await.expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, "h2");
}
```

- [ ] **Step 2: テストが失敗することを確認**

```bash
cd /Users/ittan/Asweed/sandbox/Sugo
cargo test -p sugo-infra list_excludes_trashed 2>&1 | tail -5
```

Expected: FAIL（`deleted_at` カラムがない、または `list()` がフィルタしていない）

- [ ] **Step 3: `init()` に idempotent マイグレーションを追加**

`sugo-infra/src/sqlite/repository.rs` の `inject_pending_since` マイグレーションブロック（約103行目）の直後に追加：

```rust
        // Idempotent migration for deleted_at (soft-delete trash, added 2026-06).
        let has_deleted_at: bool = conn
            .prepare("PRAGMA table_info(harnesses)")
            .and_then(|mut s| {
                let cols = s
                    .query_map([], |row| row.get::<_, String>(1))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(cols.iter().any(|c| c == "deleted_at"))
            })
            .map_err(map_err)?;
        if !has_deleted_at {
            conn.execute(
                "ALTER TABLE harnesses ADD COLUMN deleted_at TEXT",
                [],
            )
            .map_err(map_err)?;
        }
```

- [ ] **Step 4: `list()` に `WHERE deleted_at IS NULL` を追加**

`sugo-infra/src/sqlite/repository.rs` の `list()` 実装を変更：

```rust
    async fn list(&self) -> Result<Vec<Harness>, CoreError> {
        let conn = self.lock();
        let mut stmt = conn
            .prepare(
                "SELECT id,name,current_version,has_draft,lock_version,created_at,updated_at \
                 FROM harnesses WHERE deleted_at IS NULL",
            )
            .map_err(map_err)?;
        let rows = stmt
            .query_map([], row_to_harness)
            .map_err(map_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_err)?;
        Ok(rows)
    }
```

- [ ] **Step 5: マイグレーションのべき等テストを追加**

既存の `migration_adds_heartbeat_column_idempotently` テストの直後に追加：

```rust
    #[test]
    fn migration_adds_deleted_at_column_idempotently() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("m_del.db");
        let p = path.to_str().unwrap();
        let _r1 = SqliteHarnessRepository::open(p).unwrap();
        let _r2 = SqliteHarnessRepository::open(p).unwrap();
        let conn = rusqlite::Connection::open(p).unwrap();
        let mut s = conn.prepare("PRAGMA table_info(harnesses)").unwrap();
        let cols: Vec<String> = s
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .map(|c| c.unwrap())
            .collect();
        assert!(cols.iter().any(|c| c == "deleted_at"));
    }
```

- [ ] **Step 6: テストが通ることを確認**

```bash
cd /Users/ittan/Asweed/sandbox/Sugo
cargo test -p sugo-infra 2>&1 | tail -10
```

Expected: すべて PASS

- [ ] **Step 7: コミット**

```bash
git add sugo-infra/src/sqlite/repository.rs
git commit -m "feat(infra): harnesses.deleted_at マイグレーション + list() ソフトデリートフィルタ"
```

---

### Task 2: Backend – `HarnessRepository` トレイトにゴミ箱メソッドを追加

**Files:**
- Modify: `sugo-core/src/ports/repository.rs`

**Interfaces:**
- Produces:
  - `HarnessRepository::trash_harness(id: &str, deleted_at: &str) -> Result<(), CoreError>`
  - `HarnessRepository::restore_harness(id: &str) -> Result<(), CoreError>`
  - `HarnessRepository::purge_harness(id: &str) -> Result<(), CoreError>`
  - `HarnessRepository::list_trash() -> Result<Vec<(String, String, String)>, CoreError>` → `(id, name, deleted_at)`
  - `HarnessRepository::purge_trash_before(before_iso: &str) -> Result<(), CoreError>`
- Produces: `InMemoryHarnessRepository` に `deleted_at: Mutex<HashMap<String, String>>` フィールド追加、`list()` がフィルタ済み

- [ ] **Step 1: トレイトにメソッドを追加**

`sugo-core/src/ports/repository.rs` の `HarnessRepository` トレイト（`append_version` の直後）に追加：

```rust
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
```

- [ ] **Step 2: `InMemoryHarnessRepository` に `deleted_at` フィールドを追加**

`InMemoryHarnessRepository` 構造体定義を変更：

```rust
    #[derive(Default)]
    pub struct InMemoryHarnessRepository {
        harnesses: Mutex<HashMap<String, Harness>>,
        versions: Mutex<HashMap<(String, i64), BoardVersion>>,
        deleted_at: Mutex<HashMap<String, String>>,
    }
```

- [ ] **Step 3: `list()` の fake 実装を更新**

`InMemoryHarnessRepository` の `list()` 実装を変更：

```rust
        async fn list(&self) -> Result<Vec<Harness>, CoreError> {
            let hs = self.harnesses.lock().unwrap();
            let deleted = self.deleted_at.lock().unwrap();
            Ok(hs
                .values()
                .filter(|h| !deleted.contains_key(&h.id))
                .cloned()
                .collect())
        }
```

- [ ] **Step 4: ゴミ箱メソッドの fake 実装を追加**

`InMemoryHarnessRepository` の `impl HarnessRepository` ブロックに追加（`append_version` の直後）：

```rust
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
```

- [ ] **Step 5: コンパイルエラーがないことを確認**

```bash
cd /Users/ittan/Asweed/sandbox/Sugo
cargo check -p sugo-core 2>&1 | tail -10
```

Expected: `Finished` （エラーなし）

- [ ] **Step 6: コミット**

```bash
git add sugo-core/src/ports/repository.rs
git commit -m "feat(core): HarnessRepository にゴミ箱トレイトメソッドを追加"
```

---

### Task 3: Backend – SQLite ゴミ箱メソッド実装

**Files:**
- Modify: `sugo-infra/src/sqlite/repository.rs`

**Interfaces:**
- Consumes: Task 2 で追加したトレイトメソッド群
- Produces: `SqliteHarnessRepository` が全トレイトメソッドを実装

- [ ] **Step 1: テストを書く**

`sugo-infra/src/sqlite/repository.rs` のテストモジュール末尾に追加：

```rust
    #[tokio::test]
    async fn trash_and_restore_roundtrip() {
        let repo = SqliteHarnessRepository::in_memory().expect("in-memory");
        repo.create(&harness("h1", 1, 0), &version("v1", "h1", 1, "p"))
            .await
            .expect("seed");

        // trash
        repo.trash_harness("h1", "2026-06-30T10:00:00+09:00")
            .await
            .expect("trash");
        assert!(repo.list().await.unwrap().is_empty(), "list excludes trashed");
        let trash = repo.list_trash().await.unwrap();
        assert_eq!(trash.len(), 1);
        assert_eq!(trash[0].0, "h1");

        // restore
        repo.restore_harness("h1").await.expect("restore");
        assert_eq!(repo.list().await.unwrap().len(), 1);
        assert!(repo.list_trash().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn purge_harness_removes_all() {
        let repo = SqliteHarnessRepository::in_memory().expect("in-memory");
        repo.create(&harness("h1", 1, 0), &version("v1", "h1", 1, "p"))
            .await
            .expect("seed");
        repo.trash_harness("h1", "2026-06-30T10:00:00+09:00")
            .await
            .expect("trash");
        repo.purge_harness("h1").await.expect("purge");
        assert!(repo.list().await.unwrap().is_empty());
        assert!(repo.list_trash().await.unwrap().is_empty());
        // board_versions も消えている
        assert!(repo.get_version("h1", 1).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn purge_trash_before_removes_old_only() {
        let repo = SqliteHarnessRepository::in_memory().expect("in-memory");
        repo.create(&harness("old", 1, 0), &version("v-old", "old", 1, "p"))
            .await
            .expect("seed old");
        repo.create(&harness("new", 1, 0), &version("v-new", "new", 1, "p"))
            .await
            .expect("seed new");
        repo.trash_harness("old", "2025-12-01T00:00:00+09:00")
            .await
            .expect("trash old");
        repo.trash_harness("new", "2026-06-29T00:00:00+09:00")
            .await
            .expect("trash new");
        // cutoff: 2026-06-01 → old は削除、new は残る
        repo.purge_trash_before("2026-06-01T00:00:00+09:00")
            .await
            .expect("auto-purge");
        let trash = repo.list_trash().await.unwrap();
        assert_eq!(trash.len(), 1);
        assert_eq!(trash[0].0, "new");
    }
```

- [ ] **Step 2: テストが失敗することを確認**

```bash
cd /Users/ittan/Asweed/sandbox/Sugo
cargo test -p sugo-infra trash_and_restore 2>&1 | tail -5
```

Expected: FAIL（メソッド未実装）

- [ ] **Step 3: SQLite 実装を追加**

`sugo-infra/src/sqlite/repository.rs` の `impl HarnessRepository for SqliteHarnessRepository` ブロック（`append_version` の直後）に追加：

```rust
    async fn trash_harness(&self, id: &str, deleted_at: &str) -> Result<(), CoreError> {
        let conn = self.lock();
        let affected = conn
            .execute(
                "UPDATE harnesses SET deleted_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
                rusqlite::params![deleted_at, id],
            )
            .map_err(map_err)?;
        if affected == 0 {
            Err(CoreError::NotFound(id.to_string()))
        } else {
            Ok(())
        }
    }

    async fn restore_harness(&self, id: &str) -> Result<(), CoreError> {
        let conn = self.lock();
        let affected = conn
            .execute(
                "UPDATE harnesses SET deleted_at = NULL WHERE id = ?1 AND deleted_at IS NOT NULL",
                [id],
            )
            .map_err(map_err)?;
        if affected == 0 {
            Err(CoreError::NotFound(id.to_string()))
        } else {
            Ok(())
        }
    }

    async fn purge_harness(&self, id: &str) -> Result<(), CoreError> {
        let mut conn = self.lock();
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(map_err)?;
        tx.execute("DELETE FROM board_versions WHERE harness_id = ?1", [id])
            .map_err(map_err)?;
        tx.execute("DELETE FROM runs WHERE harness_id = ?1", [id])
            .map_err(map_err)?;
        let affected = tx
            .execute("DELETE FROM harnesses WHERE id = ?1", [id])
            .map_err(map_err)?;
        tx.commit().map_err(map_err)?;
        if affected == 0 {
            Err(CoreError::NotFound(id.to_string()))
        } else {
            Ok(())
        }
    }

    async fn list_trash(&self) -> Result<Vec<(String, String, String)>, CoreError> {
        let conn = self.lock();
        let mut stmt = conn
            .prepare(
                "SELECT id, name, deleted_at FROM harnesses \
                 WHERE deleted_at IS NOT NULL ORDER BY deleted_at DESC",
            )
            .map_err(map_err)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(map_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_err)?;
        Ok(rows)
    }

    async fn purge_trash_before(&self, before_iso: &str) -> Result<(), CoreError> {
        let to_purge: Vec<String> = {
            let conn = self.lock();
            let mut stmt = conn
                .prepare(
                    "SELECT id FROM harnesses \
                     WHERE deleted_at IS NOT NULL AND deleted_at < ?1",
                )
                .map_err(map_err)?;
            stmt.query_map([before_iso], |row| row.get(0))
                .map_err(map_err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(map_err)?
        };
        for id in to_purge {
            self.purge_harness(&id).await?;
        }
        Ok(())
    }
```

- [ ] **Step 4: テストが通ることを確認**

```bash
cd /Users/ittan/Asweed/sandbox/Sugo
cargo test -p sugo-infra 2>&1 | tail -10
```

Expected: すべて PASS

- [ ] **Step 5: コミット**

```bash
git add sugo-infra/src/sqlite/repository.rs
git commit -m "feat(infra): SqliteHarnessRepository にゴミ箱 SQL メソッドを実装"
```

---

### Task 4: Backend – Tauri コマンド追加 + 起動時自動パージ

**Files:**
- Modify: `sugo-gui/src-tauri/src/dto.rs`
- Modify: `sugo-gui/src-tauri/src/commands.rs`
- Modify: `sugo-gui/src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: Task 3 の `trash_harness / restore_harness / purge_harness / list_trash / purge_trash_before`
- Produces: Tauri コマンド `trash_harness / restore_harness / purge_harness / list_trash`（フロントから `invoke` で呼べる）
- Produces: アプリ起動時に 180 日超のゴミ箱を自動パージ

- [ ] **Step 1: `TrashItemDto` を追加**

`sugo-gui/src-tauri/src/dto.rs` 末尾に追加：

```rust
#[derive(Debug, Serialize)]
pub struct TrashItemDto {
    pub harness_id: String,
    pub name: String,
    pub deleted_at: String,
    pub remaining_days: i64,
}
```

- [ ] **Step 2: コマンドのテストを書く**

`sugo-gui/src-tauri/src/commands.rs` のテストモジュール末尾に追加：

```rust
    // ── trash_harness（inner）────────────────────────────────────────────────

    async fn seed_harness(repo: &InMemoryHarnessRepository) -> String {
        let clock = FakeIdClock::new();
        create_harness(
            repo,
            &clock,
            CreateHarnessInput { name: "h".into(), definition: None },
        )
        .await
        .unwrap()
        .harness_id
    }

    #[tokio::test]
    async fn trash_harness_inner_moves_harness_to_trash() {
        let repo = InMemoryHarnessRepository::new();
        let hid = seed_harness(&repo).await;

        trash_harness_inner(&repo, hid.clone(), "2026-06-30T10:00:00+09:00".into())
            .await
            .unwrap();
        // list() excludes trashed
        assert!(repo.list().await.unwrap().is_empty());
        // list_trash() includes it
        let trash = repo.list_trash().await.unwrap();
        assert_eq!(trash.len(), 1);
        assert_eq!(trash[0].0, hid);
    }

    #[tokio::test]
    async fn trash_harness_inner_unknown_returns_not_found() {
        let repo = InMemoryHarnessRepository::new();
        let err = trash_harness_inner(&repo, "ghost".into(), "2026-06-30T10:00:00+09:00".into())
            .await
            .unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
    }

    #[tokio::test]
    async fn restore_harness_inner_moves_back_to_active() {
        let repo = InMemoryHarnessRepository::new();
        let hid = seed_harness(&repo).await;
        repo.trash_harness(&hid, "2026-06-30T10:00:00+09:00")
            .await
            .unwrap();

        restore_harness_inner(&repo, hid.clone()).await.unwrap();
        assert_eq!(repo.list().await.unwrap().len(), 1);
        assert!(repo.list_trash().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn purge_harness_inner_removes_harness() {
        let repo = InMemoryHarnessRepository::new();
        let hid = seed_harness(&repo).await;
        repo.trash_harness(&hid, "2026-06-30T10:00:00+09:00")
            .await
            .unwrap();

        purge_harness_inner(&repo, hid.clone()).await.unwrap();
        assert!(repo.list_trash().await.unwrap().is_empty());
        assert!(repo.get(&hid).await.unwrap().is_none());
    }
```

- [ ] **Step 3: テストが失敗することを確認**

```bash
cd /Users/ittan/Asweed/sandbox/Sugo
cargo test -p sugo-gui 2>&1 | tail -10
```

Expected: FAIL（inner 関数未定義）

- [ ] **Step 4: コマンドを実装**

`sugo-gui/src-tauri/src/commands.rs` の先頭 `use` 群に追加：

```rust
use crate::dto::TrashItemDto;
use sugo_core::domain::run::RunStatus;
```

`get_active_runs` コマンドの直後に追加：

```rust
pub(crate) async fn trash_harness_inner(
    repo: &dyn HarnessRepository,
    harness_id: String,
    deleted_at: String,
) -> Result<(), String> {
    repo.trash_harness(&harness_id, &deleted_at)
        .await
        .map_err(|e| e.to_string())
}

pub(crate) async fn restore_harness_inner(
    repo: &dyn HarnessRepository,
    harness_id: String,
) -> Result<(), String> {
    repo.restore_harness(&harness_id)
        .await
        .map_err(|e| e.to_string())
}

pub(crate) async fn purge_harness_inner(
    repo: &dyn HarnessRepository,
    harness_id: String,
) -> Result<(), String> {
    repo.purge_harness(&harness_id)
        .await
        .map_err(|e| e.to_string())
}

/// ハーネスをゴミ箱に移動する。アクティブ Run が存在する場合は `"active_run"` エラーを返す。
#[tauri::command]
pub async fn trash_harness(
    state: State<'_, AppState>,
    harness_id: String,
) -> Result<(), String> {
    // アクティブ Run チェック（get_active_runs と同じ判定）
    let runs = state
        .run_repo
        .list_by_harness(&harness_id)
        .await
        .map_err(|e| e.to_string())?;
    let now = chrono::Utc::now();
    let stale_secs = chrono::Duration::seconds(300);
    let has_active = runs
        .iter()
        .filter(|r| r.status == RunStatus::Running)
        .any(|r| {
            let ts = r.last_heartbeat_at.as_deref().unwrap_or(&r.updated_at);
            chrono::DateTime::parse_from_rfc3339(ts)
                .map(|dt| {
                    now.signed_duration_since(dt.with_timezone(&chrono::Utc)) < stale_secs
                })
                .unwrap_or(false)
        });
    if has_active {
        return Err("active_run".to_string());
    }
    let deleted_at = chrono::Local::now().to_rfc3339();
    trash_harness_inner(state.repo.as_ref(), harness_id, deleted_at).await
}

/// ゴミ箱からハーネスを復活させる。
#[tauri::command]
pub async fn restore_harness(
    state: State<'_, AppState>,
    harness_id: String,
) -> Result<(), String> {
    restore_harness_inner(state.repo.as_ref(), harness_id).await
}

/// ハーネスを物理削除する（ゴミ箱からのみ呼ぶ）。
#[tauri::command]
pub async fn purge_harness(
    state: State<'_, AppState>,
    harness_id: String,
) -> Result<(), String> {
    purge_harness_inner(state.repo.as_ref(), harness_id).await
}

/// ゴミ箱一覧取得（残り日数付き）。
#[tauri::command]
pub async fn list_trash(
    state: State<'_, AppState>,
) -> Result<Vec<TrashItemDto>, String> {
    let items = state.repo.list_trash().await.map_err(|e| e.to_string())?;
    let now = chrono::Local::now();
    Ok(items
        .into_iter()
        .map(|(id, name, deleted_at)| {
            let remaining_days = chrono::DateTime::parse_from_rfc3339(&deleted_at)
                .map(|dt| {
                    let elapsed =
                        now.signed_duration_since(dt.with_timezone(&chrono::Local));
                    (180 - elapsed.num_days()).max(0)
                })
                .unwrap_or(0);
            TrashItemDto {
                harness_id: id,
                name,
                deleted_at,
                remaining_days,
            }
        })
        .collect())
}
```

`commands.rs` 先頭の `use crate::dto::{...}` に `TrashItemDto` を追加：

```rust
use crate::dto::{ActiveRunDto, AddCellResultDto, CellDto, DeleteCellResultDto, DraftCellDto, EdgeDto, HarnessDetailDto, HarnessSummaryDto, RenameCellResultDto, TrashItemDto};
```

- [ ] **Step 5: テストモジュールの import を確認・補完**

`commands.rs` のテストモジュール内 `use super::*;` の直後に不足があれば追加（既存 import を流用するので変更不要なはず）。実際に `trash_harness_inner`, `restore_harness_inner`, `purge_harness_inner` はモジュール内の公開 fn なので `use super::*;` でアクセス可能。

- [ ] **Step 6: `lib.rs` にコマンド登録と自動パージを追加**

`sugo-gui/src-tauri/src/lib.rs` を以下に変更：

```rust
mod commands;
mod dto;
mod state;

use commands::{
    add_cell, delete_cell, get_active_runs, get_harness, list_harnesses, rename_cell,
    trash_harness, restore_harness, purge_harness, list_trash,
};
use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let db_path =
                sugo_infra::paths::default_db_path().expect("resolve db path");
            let state = AppState::new(db_path.to_str().unwrap()).expect("init db");

            // 180日を超えてゴミ箱に入ったハーネスを起動時に自動パージ
            let before = (chrono::Local::now() - chrono::Duration::days(180))
                .to_rfc3339();
            let repo = state.repo.clone();
            tauri::async_runtime::block_on(async move {
                let _ = repo.purge_trash_before(&before).await;
            });

            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_harnesses,
            get_harness,
            add_cell,
            rename_cell,
            delete_cell,
            get_active_runs,
            trash_harness,
            restore_harness,
            purge_harness,
            list_trash,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 7: テストが通ることを確認**

```bash
cd /Users/ittan/Asweed/sandbox/Sugo
cargo test -p sugo-gui 2>&1 | tail -10
```

Expected: すべて PASS

- [ ] **Step 8: ビルドが通ることを確認**

```bash
cd /Users/ittan/Asweed/sandbox/Sugo
cargo build -p sugo-gui 2>&1 | tail -10
```

Expected: `Finished`

- [ ] **Step 9: コミット**

```bash
git add sugo-gui/src-tauri/src/dto.rs sugo-gui/src-tauri/src/commands.rs sugo-gui/src-tauri/src/lib.rs
git commit -m "feat(gui): ゴミ箱 Tauri コマンド追加 + 起動時自動パージ"
```

---

### Task 5: Frontend – @heroicons/vue インストール + App.vue + Router 更新

**Files:**
- Modify: `sugo-gui/package.json` (install)
- Modify: `sugo-gui/src/App.vue`
- Modify: `sugo-gui/src/router/index.ts`

**Interfaces:**
- Produces: `@heroicons/vue/24/outline` が import 可能
- Produces: `/` と `/trash` が ShellLayout 配下のネストルートに
- Produces: App.vue の `<main>` から `p-4` を除去（各ビューが自前でパディングを持つ）

- [ ] **Step 1: @heroicons/vue をインストール**

```bash
cd /Users/ittan/Asweed/sandbox/Sugo/sugo-gui && npm install @heroicons/vue
```

Expected: `added N packages` のような出力、`package.json` の dependencies に `@heroicons/vue` が追加される

- [ ] **Step 2: App.vue の `<main>` から `p-4` を除去**

`sugo-gui/src/App.vue` の `<main>` タグを変更：

変更前：
```html
    <main class="flex-1 overflow-hidden flex flex-col p-4">
```

変更後：
```html
    <main class="flex-1 overflow-hidden flex flex-col">
```

- [ ] **Step 3: HarnessView.vue に `p-4` を追加**

`sugo-gui/src/views/HarnessView.vue` の最外ラッパを変更：

変更前：
```html
  <div v-if="detail" class="flex flex-col h-full">
```

変更後：
```html
  <div v-if="detail" class="flex flex-col h-full p-4">
```

- [ ] **Step 4: Router をネスト構成に更新**

`sugo-gui/src/router/index.ts` を以下に変更：

```typescript
import { createRouter, createWebHistory } from "vue-router";
import ShellLayout from "../layouts/ShellLayout.vue";
import HarnessList from "../views/HarnessList.vue";
import TrashView from "../views/TrashView.vue";
import HarnessView from "../views/HarnessView.vue";

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: "/",
      component: ShellLayout,
      children: [
        { path: "", component: HarnessList },
        { path: "trash", component: TrashView },
      ],
    },
    { path: "/harness/:id", component: HarnessView, props: true },
  ],
});
```

- [ ] **Step 5: HarnessList 既存テストが通ることを確認**

（ShellLayout はまだ未作成でよい。テストは HarnessList を直接 mount するため影響なし）

```bash
cd /Users/ittan/Asweed/sandbox/Sugo/sugo-gui && npm test 2>&1 | tail -15
```

Expected: PASS（既存テストはそのまま通る）

- [ ] **Step 6: コミット**

```bash
git add sugo-gui/package.json sugo-gui/package-lock.json sugo-gui/src/App.vue sugo-gui/src/router/index.ts sugo-gui/src/views/HarnessView.vue
git commit -m "feat(gui): @heroicons/vue インストール + ルーター ShellLayout ネスト化"
```

---

### Task 6: Frontend – ShellLayout.vue + AppSidebar.vue 作成

**Files:**
- Create: `sugo-gui/src/layouts/ShellLayout.vue`
- Create: `sugo-gui/src/components/AppSidebar.vue`

**Interfaces:**
- Consumes: `@heroicons/vue/24/outline` の `TrashIcon`, `ListBulletIcon`
- Consumes: Tauri コマンド `list_trash` (count 取得用)
- Produces: ShellLayout が `/` と `/trash` をラップする2カラムレイアウト
- Produces: AppSidebar が 2秒ポーリングでゴミ箱バッジ数を更新

- [ ] **Step 1: `src/layouts/` ディレクトリ作成確認**

```bash
ls /Users/ittan/Asweed/sandbox/Sugo/sugo-gui/src/
```

`layouts/` ディレクトリがなければ次のステップで `ShellLayout.vue` を作成すれば自動作成される（Write ツールが作成する）

- [ ] **Step 2: ShellLayout.vue を作成**

`sugo-gui/src/layouts/ShellLayout.vue` を新規作成：

```vue
<template>
  <div class="flex h-full overflow-hidden">
    <AppSidebar />
    <div class="flex-1 overflow-auto p-4">
      <RouterView />
    </div>
  </div>
</template>

<script setup lang="ts">
import AppSidebar from "../components/AppSidebar.vue";
</script>
```

- [ ] **Step 3: AppSidebar.vue を作成**

`sugo-gui/src/components/AppSidebar.vue` を新規作成：

```vue
<template>
  <nav class="w-[140px] shrink-0 bg-white border-r border-gray-200 flex flex-col py-2 gap-0.5">
    <RouterLink
      to="/"
      exact-active-class="bg-gray-100 font-medium"
      class="flex items-center gap-2 px-3 py-2 text-sm rounded mx-1 hover:bg-gray-100 text-gray-700"
    >
      <ListBulletIcon class="w-4 h-4 shrink-0" />
      ハーネス
    </RouterLink>
    <RouterLink
      to="/trash"
      active-class="bg-gray-100 font-medium"
      class="flex items-center gap-2 px-3 py-2 text-sm rounded mx-1 hover:bg-gray-100 text-gray-700"
    >
      <TrashIcon class="w-4 h-4 shrink-0" />
      ゴミ箱
      <span
        v-if="trashCount > 0"
        class="ml-auto text-xs bg-gray-200 text-gray-600 rounded-full px-1.5 py-0.5 leading-none"
      >
        {{ trashCount }}
      </span>
    </RouterLink>
  </nav>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { TrashIcon, ListBulletIcon } from "@heroicons/vue/24/outline";

const POLL_INTERVAL_MS = 2000;
const trashCount = ref(0);
let pollTimer: ReturnType<typeof setInterval> | null = null;

async function fetchCount() {
  const items = await invoke<{ harness_id: string }[]>("list_trash");
  trashCount.value = items.length;
}

onMounted(() => {
  void fetchCount();
  pollTimer = setInterval(fetchCount, POLL_INTERVAL_MS);
});
onUnmounted(() => {
  if (pollTimer) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
});
</script>
```

- [ ] **Step 4: TypeScript エラーがないことを確認**

```bash
cd /Users/ittan/Asweed/sandbox/Sugo/sugo-gui && npm run build 2>&1 | tail -15
```

Expected: ビルド成功

- [ ] **Step 5: コミット**

```bash
git add sugo-gui/src/layouts/ShellLayout.vue sugo-gui/src/components/AppSidebar.vue
git commit -m "feat(gui): ShellLayout + AppSidebar を追加"
```

---

### Task 7: Frontend – HarnessList.vue 更新（ホバーゴミ箱アイコン + 確認ダイアログ）

**Files:**
- Modify: `sugo-gui/src/views/HarnessList.vue`
- Modify: `sugo-gui/src/views/HarnessList.test.ts`

**Interfaces:**
- Consumes: Tauri コマンド `trash_harness(harnessId: string)`（エラー `"active_run"` を含む）
- Produces: 行ホバーで TrashIcon が出現、クリックで確認ダイアログ表示、確認後に trash_harness を呼ぶ

- [ ] **Step 1: テストを追加**

`sugo-gui/src/views/HarnessList.test.ts` に追加：

```typescript
import { describe, it, expect, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { createRouter, createMemoryHistory } from "vue-router";
import HarnessList from "./HarnessList.vue";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue([
    { harness_id: "h1", name: "alpha", current_version: 1, has_draft: false },
    { harness_id: "h2", name: "beta", current_version: 2, has_draft: true },
  ]),
}));

function makeRouter() {
  return createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/", component: HarnessList },
      { path: "/harness/:id", component: { template: "<div/>" } },
    ],
  });
}

// 既存テストはここに残す（変更なし）

describe("HarnessList – trash confirmation", () => {
  it("shows confirmation dialog when trash icon clicked", async () => {
    const wrapper = mount(HarnessList, { global: { plugins: [makeRouter()] } });
    await new Promise((r) => setTimeout(r, 0));
    // ダイアログは初期非表示
    expect(wrapper.find("[data-testid='trash-dialog']").exists()).toBe(false);
    // ゴミ箱ボタンをクリック（h1 の行）
    await wrapper.findAll("[data-testid='trash-btn']")[0].trigger("click");
    expect(wrapper.find("[data-testid='trash-dialog']").exists()).toBe(true);
    expect(wrapper.text()).toContain("alpha");
  });

  it("hides dialog on cancel", async () => {
    const wrapper = mount(HarnessList, { global: { plugins: [makeRouter()] } });
    await new Promise((r) => setTimeout(r, 0));
    await wrapper.findAll("[data-testid='trash-btn']")[0].trigger("click");
    await wrapper.find("[data-testid='trash-cancel-btn']").trigger("click");
    expect(wrapper.find("[data-testid='trash-dialog']").exists()).toBe(false);
  });
});
```

- [ ] **Step 2: テストが失敗することを確認**

```bash
cd /Users/ittan/Asweed/sandbox/Sugo/sugo-gui && npm test 2>&1 | tail -15
```

Expected: FAIL（`data-testid` 属性が未実装）

- [ ] **Step 3: HarnessList.vue を更新**

`sugo-gui/src/views/HarnessList.vue` を以下に置換：

```vue
<template>
  <div>
    <h2 class="text-lg font-semibold mb-4">ハーネス一覧</h2>
    <ul class="space-y-2">
      <li
        v-for="h in harnesses"
        :key="h.harness_id"
        class="group bg-white rounded border border-gray-200 px-4 py-3 cursor-pointer hover:bg-gray-50 flex items-center justify-between"
        @click="router.push(`/harness/${h.harness_id}`)"
      >
        <span class="font-medium">{{ h.name }}</span>
        <div class="flex items-center gap-2">
          <span class="text-sm text-gray-400">v{{ h.current_version }}</span>
          <span
            v-if="h.has_draft"
            class="text-xs bg-yellow-100 text-yellow-800 px-2 py-0.5 rounded font-bold"
          >
            DRAFT
          </span>
          <button
            data-testid="trash-btn"
            class="opacity-0 group-hover:opacity-100 p-1 text-gray-400 hover:text-red-500 transition-opacity"
            @click.stop="confirmTrash(h)"
          >
            <TrashIcon class="w-4 h-4" />
          </button>
        </div>
      </li>
    </ul>
    <p v-if="harnesses.length === 0" class="text-gray-400">
      ハーネスがありません。MCP の sugo_create_harness で作成してください。
    </p>

    <!-- 確認ダイアログ -->
    <div
      v-if="trashTarget"
      data-testid="trash-dialog"
      class="fixed inset-0 bg-black/30 z-50 flex items-center justify-center"
    >
      <div class="bg-white rounded-lg shadow-lg p-6 w-80">
        <p class="text-sm font-medium mb-4">
          "{{ trashTarget.name }}" をゴミ箱に移動しますか？
        </p>
        <p v-if="trashError" class="text-xs text-red-500 mb-3">{{ trashError }}</p>
        <div class="flex gap-2 justify-end">
          <button
            data-testid="trash-cancel-btn"
            class="px-3 py-1.5 text-sm text-gray-600 hover:text-gray-900"
            @click="trashTarget = null; trashError = null"
          >
            キャンセル
          </button>
          <button
            class="px-3 py-1.5 text-sm bg-red-500 text-white rounded hover:bg-red-600"
            @click="doTrash"
          >
            移動する
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useRouter } from "vue-router";
import { TrashIcon } from "@heroicons/vue/24/outline";

interface HarnessSummary {
  harness_id: string;
  name: string;
  current_version: number;
  has_draft: boolean;
}

const POLL_INTERVAL_MS = 2000;
const router = useRouter();
const harnesses = ref<HarnessSummary[]>([]);
const trashTarget = ref<HarnessSummary | null>(null);
const trashError = ref<string | null>(null);
let pollTimer: ReturnType<typeof setInterval> | null = null;

async function fetchHarnesses() {
  harnesses.value = await invoke<HarnessSummary[]>("list_harnesses");
}

function confirmTrash(h: HarnessSummary) {
  trashTarget.value = h;
  trashError.value = null;
}

async function doTrash() {
  if (!trashTarget.value) return;
  try {
    await invoke("trash_harness", { harnessId: trashTarget.value.harness_id });
    trashTarget.value = null;
    await fetchHarnesses();
  } catch (e: unknown) {
    trashError.value =
      e === "active_run"
        ? "実行中のRunがあるため移動できません"
        : String(e);
  }
}

onMounted(() => {
  void fetchHarnesses();
  pollTimer = setInterval(fetchHarnesses, POLL_INTERVAL_MS);
});
onUnmounted(() => {
  if (pollTimer) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
});
</script>
```

- [ ] **Step 4: テストが通ることを確認**

```bash
cd /Users/ittan/Asweed/sandbox/Sugo/sugo-gui && npm test 2>&1 | tail -15
```

Expected: すべて PASS

- [ ] **Step 5: コミット**

```bash
git add sugo-gui/src/views/HarnessList.vue sugo-gui/src/views/HarnessList.test.ts
git commit -m "feat(gui): HarnessList にホバーゴミ箱アイコンと確認ダイアログを追加"
```

---

### Task 8: Frontend – TrashView.vue 作成

**Files:**
- Create: `sugo-gui/src/views/TrashView.vue`

**Interfaces:**
- Consumes: Tauri コマンド `list_trash() → TrashItemDto[]` (`harness_id`, `name`, `deleted_at`, `remaining_days`)
- Consumes: Tauri コマンド `restore_harness(harnessId)`, `purge_harness(harnessId)`
- Produces: ゴミ箱一覧 UI（復活・完全削除ボタン付き、残り30日以内は赤字）

- [ ] **Step 1: TrashView.vue を作成**

`sugo-gui/src/views/TrashView.vue` を新規作成：

```vue
<template>
  <div>
    <h2 class="text-lg font-semibold mb-4">ゴミ箱</h2>
    <ul class="space-y-2">
      <li
        v-for="item in trashItems"
        :key="item.harness_id"
        class="bg-white rounded border border-gray-200 px-4 py-3 flex items-center justify-between"
      >
        <div>
          <p class="font-medium">{{ item.name }}</p>
          <p class="text-xs text-gray-400 mt-0.5">
            削除日: {{ formatDate(item.deleted_at) }}　
            <span :class="item.remaining_days <= 30 ? 'text-red-500 font-medium' : ''">
              あと{{ item.remaining_days }}日
            </span>
          </p>
        </div>
        <div class="flex items-center gap-2">
          <button
            class="px-3 py-1 text-sm border border-gray-300 rounded hover:bg-gray-50 text-gray-700"
            @click="restore(item)"
          >
            復活
          </button>
          <button
            class="px-3 py-1 text-sm text-red-500 border border-red-200 rounded hover:bg-red-50"
            @click="confirmPurge(item)"
          >
            完全削除
          </button>
        </div>
      </li>
    </ul>
    <p v-if="trashItems.length === 0" class="text-gray-400">ゴミ箱は空です。</p>

    <!-- 完全削除確認ダイアログ -->
    <div
      v-if="purgeTarget"
      class="fixed inset-0 bg-black/30 z-50 flex items-center justify-center"
    >
      <div class="bg-white rounded-lg shadow-lg p-6 w-80">
        <p class="text-sm font-medium mb-2">
          "{{ purgeTarget.name }}" を完全に削除しますか？
        </p>
        <p class="text-xs text-gray-500 mb-4">この操作は元に戻せません。</p>
        <div class="flex gap-2 justify-end">
          <button
            class="px-3 py-1.5 text-sm text-gray-600 hover:text-gray-900"
            @click="purgeTarget = null"
          >
            キャンセル
          </button>
          <button
            class="px-3 py-1.5 text-sm bg-red-500 text-white rounded hover:bg-red-600"
            @click="doPurge"
          >
            完全削除
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";

interface TrashItem {
  harness_id: string;
  name: string;
  deleted_at: string;
  remaining_days: number;
}

const POLL_INTERVAL_MS = 2000;
const trashItems = ref<TrashItem[]>([]);
const purgeTarget = ref<TrashItem | null>(null);
let pollTimer: ReturnType<typeof setInterval> | null = null;

async function fetchTrash() {
  trashItems.value = await invoke<TrashItem[]>("list_trash");
}

async function restore(item: TrashItem) {
  await invoke("restore_harness", { harnessId: item.harness_id });
  await fetchTrash();
}

function confirmPurge(item: TrashItem) {
  purgeTarget.value = item;
}

async function doPurge() {
  if (!purgeTarget.value) return;
  await invoke("purge_harness", { harnessId: purgeTarget.value.harness_id });
  purgeTarget.value = null;
  await fetchTrash();
}

function formatDate(iso: string): string {
  return iso.slice(0, 10);
}

onMounted(() => {
  void fetchTrash();
  pollTimer = setInterval(fetchTrash, POLL_INTERVAL_MS);
});
onUnmounted(() => {
  if (pollTimer) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
});
</script>
```

- [ ] **Step 2: TypeScript エラーがないことを確認**

```bash
cd /Users/ittan/Asweed/sandbox/Sugo/sugo-gui && npm run build 2>&1 | tail -10
```

Expected: ビルド成功

- [ ] **Step 3: テストが通ることを確認**

```bash
cd /Users/ittan/Asweed/sandbox/Sugo/sugo-gui && npm test 2>&1 | tail -10
```

Expected: すべて PASS

- [ ] **Step 4: コミット**

```bash
git add sugo-gui/src/views/TrashView.vue
git commit -m "feat(gui): TrashView（復活・完全削除・自動削除残日数表示）を追加"
```

---

## 完了チェックリスト（実装後に確認）

- [ ] `cargo test` が全パッケージで PASS
- [ ] `npm test` が PASS
- [ ] dev サーバー起動後、ハーネス一覧左にサイドバーが表示される
- [ ] ハーネス行にホバーするとゴミ箱アイコン（SVG）が出る
- [ ] 確認ダイアログが表示され「移動する」でゴミ箱へ
- [ ] ゴミ箱画面に削除日・残り日数が表示される
- [ ] 「復活」でアクティブ一覧に戻る
- [ ] 「完全削除」の確認ダイアログが出て削除される
- [ ] サイドバーのゴミ箱バッジ数が正しく更新される
