use crate::dto::{AddCellResultDto, CellDto, DraftCellDto, EdgeDto, HarnessDetailDto, HarnessSummaryDto, RenameCellResultDto};
use crate::state::AppState;
use sugo_core::domain::cell::{Cell, CellStatus};
use sugo_core::domain::harness::BoardVersion;
use sugo_core::error::CoreError;
use sugo_core::ports::repository::HarnessRepository;
use sugo_core::usecase::create_harness::content_hash;
use sugo_core::usecase::get_status::get_status;
use tauri::State;

/// ハーネス一覧取得
#[tauri::command]
pub async fn list_harnesses(
    state: State<'_, AppState>,
) -> Result<Vec<HarnessSummaryDto>, String> {
    let harnesses = state.repo.list().await.map_err(|e| e.to_string())?;
    Ok(harnesses
        .into_iter()
        .map(|h| HarnessSummaryDto {
            harness_id: h.id,
            name: h.name,
            current_version: h.current_version,
            has_draft: h.has_draft,
        })
        .collect())
}

/// ハーネス詳細取得（グラフ描画用）
#[tauri::command]
pub async fn get_harness(
    state: State<'_, AppState>,
    harness_id: String,
) -> Result<HarnessDetailDto, String> {
    let status = get_status(state.repo.as_ref(), &harness_id)
        .await
        .map_err(|e| e.to_string())?;

    let cells = status
        .definition
        .cells
        .iter()
        .map(|c| CellDto {
            id: c.id.clone(),
            name: c.name.clone(),
            prompt: c.prompt.clone(),
            status: match c.status {
                CellStatus::Active => "active".to_string(),
                CellStatus::Draft => "draft".to_string(),
            },
            terminal: c.terminal,
        })
        .collect();

    let edges = status
        .definition
        .edges
        .iter()
        .map(|e| EdgeDto {
            from: e.from.clone(),
            to: e.to.clone(),
            label: e.label.clone(),
            guard: e.guard.as_ref().map(|g| g.expr.clone()),
        })
        .collect();

    let draft_diff = status
        .draft_diff
        .iter()
        .map(|d| DraftCellDto {
            cell_id: d.cell_id.clone(),
            name: d.name.clone(),
        })
        .collect();

    // lock_version は HarnessStatus に含まれないため repo.get() で取得する
    let lock_version = state
        .repo
        .get(&harness_id)
        .await
        .map_err(|e| e.to_string())?
        .map(|(h, _)| h.lock_version)
        .unwrap_or(0);

    Ok(HarnessDetailDto {
        harness_id: status.harness_id,
        name: status.name,
        current_version: status.current_version,
        lock_version,
        has_draft: status.has_draft,
        cells,
        edges,
        draft_diff,
    })
}

/// マス追加（status: draft で登録）
#[tauri::command]
pub async fn add_cell(
    state: State<'_, AppState>,
    harness_id: String,
    cell_name: String,
    lock_version: i64,
) -> Result<AddCellResultDto, String> {
    let (mut harness, head) = state
        .repo
        .get(&harness_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| CoreError::NotFound(harness_id.clone()).to_string())?;

    let mut new_def = head.definition.clone();
    let new_cell_id = uuid::Uuid::new_v4().to_string();
    new_def.cells.push(Cell {
        id: new_cell_id,
        name: cell_name,
        prompt: String::new(),
        status: CellStatus::Draft,
        terminal: false,
    });

    let new_version_no = harness.current_version + 1;
    let now = chrono::Local::now().to_rfc3339();
    let new_version = BoardVersion {
        id: uuid::Uuid::new_v4().to_string(),
        harness_id: harness_id.clone(),
        version_no: new_version_no,
        content_hash: content_hash(&new_def),
        definition: new_def.clone(),
        created_at: now.clone(),
    };

    harness.current_version = new_version_no;
    harness.has_draft = new_def.cells.iter().any(|c| c.status == CellStatus::Draft);
    harness.lock_version = lock_version + 1;
    harness.updated_at = now;

    state
        .repo
        .append_version(&harness, &new_version, lock_version)
        .await
        .map_err(|e| e.to_string())?;

    Ok(AddCellResultDto {
        new_version: new_version_no,
        lock_version: harness.lock_version,
    })
}

/// マスのタイトル（name）を編集する（新 board_version 生成・楽観ロック）
#[tauri::command]
pub async fn rename_cell(
    state: State<'_, AppState>,
    harness_id: String,
    cell_id: String,
    new_name: String,
    lock_version: i64,
) -> Result<RenameCellResultDto, String> {
    let trimmed = new_name.trim();
    if trimmed.is_empty() {
        return Err("empty_name".to_string());
    }

    let (mut harness, head) = state
        .repo
        .get(&harness_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| CoreError::NotFound(harness_id.clone()).to_string())?;

    let mut new_def = head.definition.clone();
    let cell = new_def
        .cells
        .iter_mut()
        .find(|c| c.id == cell_id)
        .ok_or_else(|| CoreError::NotFound(cell_id.clone()).to_string())?;
    cell.name = trimmed.to_string();

    let new_version_no = harness.current_version + 1;
    let now = chrono::Local::now().to_rfc3339();
    let new_version = BoardVersion {
        id: uuid::Uuid::new_v4().to_string(),
        harness_id: harness_id.clone(),
        version_no: new_version_no,
        content_hash: content_hash(&new_def),
        definition: new_def.clone(),
        created_at: now.clone(),
    };

    harness.current_version = new_version_no;
    harness.has_draft = new_def.cells.iter().any(|c| c.status == CellStatus::Draft);
    harness.lock_version = lock_version + 1;
    harness.updated_at = now;

    state
        .repo
        .append_version(&harness, &new_version, lock_version)
        .await
        .map_err(|e| e.to_string())?;

    Ok(RenameCellResultDto {
        new_version: new_version_no,
        lock_version: harness.lock_version,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use sugo_core::domain::board::BoardDefinition;
    use sugo_core::domain::cell::{Cell, CellStatus};
    use sugo_core::domain::harness::BoardVersion;
    use sugo_core::error::CoreError;
    use sugo_core::ports::repository::fake::{FakeIdClock, InMemoryHarnessRepository};
    use sugo_core::ports::repository::HarnessRepository;
    use sugo_core::usecase::create_harness::{content_hash, create_harness, CreateHarnessInput};
    use sugo_core::usecase::get_status::get_status;

    // ── list_harnesses ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn list_harnesses_returns_all_harnesses() {
        let repo = Arc::new(InMemoryHarnessRepository::new());
        let clock = FakeIdClock::new();
        create_harness(
            repo.as_ref(),
            &clock,
            CreateHarnessInput { name: "alpha".into(), definition: None },
        )
        .await
        .unwrap();
        create_harness(
            repo.as_ref(),
            &clock,
            CreateHarnessInput { name: "beta".into(), definition: None },
        )
        .await
        .unwrap();

        let harnesses = repo.list().await.unwrap();
        assert_eq!(harnesses.len(), 2);
        let names: Vec<_> = harnesses.iter().map(|h| h.name.as_str()).collect();
        assert!(names.contains(&"alpha"));
        assert!(names.contains(&"beta"));
    }

    #[tokio::test]
    async fn list_harnesses_empty_returns_empty_vec() {
        let repo = Arc::new(InMemoryHarnessRepository::new());
        let harnesses = repo.list().await.unwrap();
        assert!(harnesses.is_empty());
    }

    // ── get_harness ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn get_harness_returns_cells_and_draft_diff() {
        let repo = Arc::new(InMemoryHarnessRepository::new());
        let clock = FakeIdClock::new();

        // draft セルを含む定義でハーネスを作成
        let def = BoardDefinition {
            schema_version: 1,
            start: "c1".into(),
            cells: vec![
                Cell {
                    id: "c1".into(),
                    name: "start".into(),
                    prompt: "p1".into(),
                    status: CellStatus::Active,
                    terminal: false,
                },
                Cell {
                    id: "c2".into(),
                    name: "draft-cell".into(),
                    prompt: "".into(),
                    status: CellStatus::Draft,
                    terminal: true,
                },
            ],
            edges: vec![],
        };
        let out = create_harness(
            repo.as_ref(),
            &clock,
            CreateHarnessInput { name: "h".into(), definition: Some(def) },
        )
        .await
        .unwrap();

        let status = get_status(repo.as_ref(), &out.harness_id).await.unwrap();
        assert_eq!(status.definition.cells.len(), 2);
        // draft_diff に c2 が含まれる（全セルが draft 混在の v1 = 基線なし）
        assert!(status.draft_diff.iter().any(|d| d.cell_id == "c2"));
        assert!(status.has_draft);
    }

    #[tokio::test]
    async fn get_status_definition_carries_prompt() {
        let repo = Arc::new(InMemoryHarnessRepository::new());
        let clock = FakeIdClock::new();
        let def = BoardDefinition {
            schema_version: 1,
            start: "c1".into(),
            cells: vec![Cell {
                id: "c1".into(),
                name: "start".into(),
                prompt: "do the thing".into(),
                status: CellStatus::Active,
                terminal: false,
            }],
            edges: vec![],
        };
        let out = create_harness(
            repo.as_ref(),
            &clock,
            CreateHarnessInput { name: "h".into(), definition: Some(def) },
        )
        .await
        .unwrap();

        let status = get_status(repo.as_ref(), &out.harness_id).await.unwrap();
        let c1 = status.definition.cells.iter().find(|c| c.id == "c1").unwrap();
        assert_eq!(c1.prompt, "do the thing");
    }

    #[tokio::test]
    async fn get_harness_not_found_returns_error() {
        let repo = Arc::new(InMemoryHarnessRepository::new());
        let result = get_status(repo.as_ref(), "nonexistent").await;
        assert!(result.is_err());
    }

    // ── add_cell ────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn add_cell_creates_draft_cell_and_bumps_version() {
        let repo = Arc::new(InMemoryHarnessRepository::new());
        let clock = FakeIdClock::new();
        let out = create_harness(
            repo.as_ref(),
            &clock,
            CreateHarnessInput { name: "h".into(), definition: None },
        )
        .await
        .unwrap();

        // lock_version は create 後 0
        let (h, _) = repo.get(&out.harness_id).await.unwrap().unwrap();
        let lv = h.lock_version;

        // add_cell をリポジトリ直接呼びでシミュレート（Tauri State なしで）
        let (mut harness, head) = repo.get(&out.harness_id).await.unwrap().unwrap();
        let mut new_def = head.definition.clone();
        new_def.cells.push(Cell {
            id: "new-c".into(),
            name: "new".into(),
            prompt: "".into(),
            status: CellStatus::Draft,
            terminal: false,
        });
        let new_version_no = harness.current_version + 1;
        let new_bv = BoardVersion {
            id: "bv-new".into(),
            harness_id: harness.id.clone(),
            version_no: new_version_no,
            content_hash: content_hash(&new_def),
            definition: new_def.clone(),
            created_at: "2026-01-01T00:00:00+09:00".into(),
        };
        harness.current_version = new_version_no;
        harness.has_draft = true;
        harness.lock_version = lv + 1;
        repo.append_version(&harness, &new_bv, lv).await.unwrap();

        // 取得して確認
        let (h2, bv2) = repo.get(&out.harness_id).await.unwrap().unwrap();
        assert_eq!(h2.current_version, 2);
        assert!(h2.has_draft);
        assert_eq!(h2.lock_version, lv + 1);
        assert_eq!(bv2.definition.cells.len(), 2); // default 1 + added 1
        assert_eq!(
            bv2.definition.cells.last().unwrap().status,
            CellStatus::Draft
        );
    }

    #[tokio::test]
    async fn add_cell_stale_lock_returns_error() {
        let repo = Arc::new(InMemoryHarnessRepository::new());
        let clock = FakeIdClock::new();
        let out = create_harness(
            repo.as_ref(),
            &clock,
            CreateHarnessInput { name: "h".into(), definition: None },
        )
        .await
        .unwrap();

        let (mut harness, head) = repo.get(&out.harness_id).await.unwrap().unwrap();
        let mut new_def = head.definition.clone();
        new_def.cells.push(Cell {
            id: "c2".into(),
            name: "two".into(),
            prompt: "".into(),
            status: CellStatus::Draft,
            terminal: false,
        });
        let new_bv = BoardVersion {
            id: "bv2".into(),
            harness_id: harness.id.clone(),
            version_no: 2,
            content_hash: content_hash(&new_def),
            definition: new_def,
            created_at: "2026-01-01T00:00:00+09:00".into(),
        };
        harness.current_version = 2;
        harness.has_draft = true;
        harness.lock_version = 1;

        // stale lock (expected=99, actual=0)
        let result = repo.append_version(&harness, &new_bv, 99).await;
        assert!(matches!(result, Err(CoreError::LockConflict { .. })));
    }

    // ── rename_cell ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn rename_cell_changes_name_only_and_bumps_version() {
        let repo = Arc::new(InMemoryHarnessRepository::new());
        let clock = FakeIdClock::new();
        let def = BoardDefinition {
            schema_version: 1,
            start: "c1".into(),
            cells: vec![Cell {
                id: "c1".into(),
                name: "old".into(),
                prompt: "p".into(),
                status: CellStatus::Active,
                terminal: false,
            }],
            edges: vec![],
        };
        let out = create_harness(
            repo.as_ref(),
            &clock,
            CreateHarnessInput { name: "h".into(), definition: Some(def) },
        )
        .await
        .unwrap();

        let (mut harness, head) = repo.get(&out.harness_id).await.unwrap().unwrap();
        let lv = harness.lock_version;
        let mut new_def = head.definition.clone();
        let cell = new_def.cells.iter_mut().find(|c| c.id == "c1").unwrap();
        cell.name = "new".into();
        let new_version_no = harness.current_version + 1;
        let new_bv = BoardVersion {
            id: "bv-rn".into(),
            harness_id: harness.id.clone(),
            version_no: new_version_no,
            content_hash: content_hash(&new_def),
            definition: new_def.clone(),
            created_at: "2026-01-01T00:00:00+09:00".into(),
        };
        harness.current_version = new_version_no;
        harness.lock_version = lv + 1;
        repo.append_version(&harness, &new_bv, lv).await.unwrap();

        let (h2, bv2) = repo.get(&out.harness_id).await.unwrap().unwrap();
        let c1 = bv2.definition.cells.iter().find(|c| c.id == "c1").unwrap();
        assert_eq!(c1.name, "new");
        assert_eq!(c1.prompt, "p"); // prompt 不変
        assert_eq!(c1.id, "c1");     // id 不変
        assert_eq!(h2.current_version, 2);
        assert_eq!(h2.lock_version, lv + 1);
    }
}
