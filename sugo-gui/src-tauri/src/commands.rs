use crate::dto::{ActiveRunDto, AddCellResultDto, AddEdgeResultDto, CellDto, DeleteCellResultDto, DeleteEdgeResultDto, DraftCellDto, EdgeDto, HarnessDetailDto, HarnessSummaryDto, RenameCellResultDto, TrashItemDto};
use crate::state::AppState;
use sugo_core::domain::cell::{Cell, CellStatus};
use sugo_core::domain::edge::{Edge, Guard};
use sugo_core::domain::harness::BoardVersion;
use sugo_core::domain::run::RunStatus;
use sugo_core::error::CoreError;
use sugo_core::ports::repository::HarnessRepository;
use sugo_core::ports::run_repository::RunRepository;
use sugo_core::usecase::create_harness::content_hash;
use sugo_core::usecase::get_status::get_status;
use tauri::State;

/// CoreError をフロントが分岐できる安定コード文字列へマップする。
///
/// 特に `LockConflict` は Display が "lock conflict: ..."（スペース区切り）で
/// あるため、フロントの判定キーと一致する安定コード `lock_conflict` を返す。
/// それ以外は Display 文字列をそのまま用いる。
fn map_core_error(e: CoreError) -> String {
    match e {
        CoreError::LockConflict { .. } => "lock_conflict".to_string(),
        other => other.to_string(),
    }
}

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

/// マス追加（status: draft で登録）。
///
/// 実体は [`add_cell_inner`] にあり、本コマンドはそれへの薄いラッパ。
/// ロジックを repo 受け取りの自由関数に切り出すことで、Tauri State 無しに
/// 単体テストできる（rename_cell と対称）。
#[tauri::command]
pub async fn add_cell(
    state: State<'_, AppState>,
    harness_id: String,
    cell_name: String,
    lock_version: i64,
) -> Result<AddCellResultDto, String> {
    add_cell_inner(state.repo.as_ref(), harness_id, cell_name, lock_version).await
}

/// `add_cell` の実体。repo を直接受け取りテスト可能にする。
async fn add_cell_inner(
    repo: &dyn HarnessRepository,
    harness_id: String,
    cell_name: String,
    lock_version: i64,
) -> Result<AddCellResultDto, String> {
    let (mut harness, head) = repo
        .get(&harness_id)
        .await
        .map_err(map_core_error)?
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

    repo.append_version(&harness, &new_version, lock_version)
        .await
        .map_err(map_core_error)?;

    Ok(AddCellResultDto {
        new_version: new_version_no,
        lock_version: harness.lock_version,
    })
}

/// マスのタイトル（name）を編集する（新 board_version 生成・楽観ロック）。
///
/// 実体は [`rename_cell_inner`] にあり、本コマンドはそれへの薄いラッパ。
/// ロジックを repo 受け取りの自由関数に切り出すことで、Tauri State 無しに
/// 単体テストできる。
#[tauri::command]
pub async fn rename_cell(
    state: State<'_, AppState>,
    harness_id: String,
    cell_id: String,
    new_name: String,
    lock_version: i64,
) -> Result<RenameCellResultDto, String> {
    rename_cell_inner(state.repo.as_ref(), harness_id, cell_id, new_name, lock_version).await
}

/// `rename_cell` の実体。repo を直接受け取りテスト可能にする。
async fn rename_cell_inner(
    repo: &dyn HarnessRepository,
    harness_id: String,
    cell_id: String,
    new_name: String,
    lock_version: i64,
) -> Result<RenameCellResultDto, String> {
    let trimmed = new_name.trim();
    if trimmed.is_empty() {
        return Err("empty_name".to_string());
    }

    let (mut harness, head) = repo
        .get(&harness_id)
        .await
        .map_err(map_core_error)?
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

    repo.append_version(&harness, &new_version, lock_version)
        .await
        .map_err(map_core_error)?;

    Ok(RenameCellResultDto {
        new_version: new_version_no,
        lock_version: harness.lock_version,
    })
}

/// draft マスを削除する（楽観ロック付き）。
/// draft 以外のマスや、そのマスへの参照エッジも同時に除去する。
#[tauri::command]
pub async fn delete_cell(
    state: State<'_, AppState>,
    harness_id: String,
    cell_id: String,
    lock_version: i64,
) -> Result<DeleteCellResultDto, String> {
    delete_cell_inner(state.repo.as_ref(), harness_id, cell_id, lock_version).await
}

async fn delete_cell_inner(
    repo: &dyn HarnessRepository,
    harness_id: String,
    cell_id: String,
    lock_version: i64,
) -> Result<DeleteCellResultDto, String> {
    let (mut harness, head) = repo
        .get(&harness_id)
        .await
        .map_err(map_core_error)?
        .ok_or_else(|| CoreError::NotFound(harness_id.clone()).to_string())?;

    let mut new_def = head.definition.clone();

    let cell = new_def
        .cells
        .iter()
        .find(|c| c.id == cell_id)
        .ok_or_else(|| CoreError::NotFound(cell_id.clone()).to_string())?;

    if cell.status != CellStatus::Draft {
        return Err("not_draft".to_string());
    }

    new_def.cells.retain(|c| c.id != cell_id);
    new_def.edges.retain(|e| e.from != cell_id && e.to != cell_id);

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

    repo.append_version(&harness, &new_version, lock_version)
        .await
        .map_err(map_core_error)?;

    Ok(DeleteCellResultDto {
        new_version: new_version_no,
        lock_version: harness.lock_version,
    })
}

/// エッジを1本追加する（新 board_version 生成・楽観ロック）。
///
/// from/to は既存セルを指すこと。空ラベルは拒否。guard は空文字なら None 扱い。
/// 実体は [`add_edge_inner`] にあり、本コマンドはそれへの薄いラッパ。
#[tauri::command]
pub async fn add_edge(
    state: State<'_, AppState>,
    harness_id: String,
    from: String,
    to: String,
    label: String,
    guard: Option<String>,
    lock_version: i64,
) -> Result<AddEdgeResultDto, String> {
    add_edge_inner(state.repo.as_ref(), harness_id, from, to, label, guard, lock_version).await
}

async fn add_edge_inner(
    repo: &dyn HarnessRepository,
    harness_id: String,
    from: String,
    to: String,
    label: String,
    guard: Option<String>,
    lock_version: i64,
) -> Result<AddEdgeResultDto, String> {
    let trimmed_label = label.trim();
    if trimmed_label.is_empty() {
        return Err("empty_label".to_string());
    }

    let (mut harness, head) = repo
        .get(&harness_id)
        .await
        .map_err(map_core_error)?
        .ok_or_else(|| CoreError::NotFound(harness_id.clone()).to_string())?;

    let mut new_def = head.definition.clone();

    // from/to は既存セルを指す必要がある
    let cell_exists = |id: &str| new_def.cells.iter().any(|c| c.id == id);
    if !cell_exists(&from) {
        return Err(CoreError::NotFound(from.clone()).to_string());
    }
    if !cell_exists(&to) {
        return Err(CoreError::NotFound(to.clone()).to_string());
    }

    // guard は空文字を None に正規化
    let guard = guard
        .map(|g| g.trim().to_string())
        .filter(|g| !g.is_empty())
        .map(|expr| Guard { expr });

    // 同一 from/to/label のエッジ重複は拒否
    let duplicate = new_def
        .edges
        .iter()
        .any(|e| e.from == from && e.to == to && e.label == trimmed_label);
    if duplicate {
        return Err("duplicate_edge".to_string());
    }

    new_def.edges.push(Edge {
        from,
        to,
        label: trimmed_label.to_string(),
        guard,
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

    repo.append_version(&harness, &new_version, lock_version)
        .await
        .map_err(map_core_error)?;

    Ok(AddEdgeResultDto {
        new_version: new_version_no,
        lock_version: harness.lock_version,
    })
}

/// エッジを1本削除する（from/to/label で一致する最初の1本、新 board_version 生成・楽観ロック）。
#[tauri::command]
pub async fn delete_edge(
    state: State<'_, AppState>,
    harness_id: String,
    from: String,
    to: String,
    label: String,
    lock_version: i64,
) -> Result<DeleteEdgeResultDto, String> {
    delete_edge_inner(state.repo.as_ref(), harness_id, from, to, label, lock_version).await
}

async fn delete_edge_inner(
    repo: &dyn HarnessRepository,
    harness_id: String,
    from: String,
    to: String,
    label: String,
    lock_version: i64,
) -> Result<DeleteEdgeResultDto, String> {
    let (mut harness, head) = repo
        .get(&harness_id)
        .await
        .map_err(map_core_error)?
        .ok_or_else(|| CoreError::NotFound(harness_id.clone()).to_string())?;

    let mut new_def = head.definition.clone();

    let before = new_def.edges.len();
    // 一致する最初の1本のみ削除する
    let mut removed = false;
    new_def.edges.retain(|e| {
        if !removed && e.from == from && e.to == to && e.label == label {
            removed = true;
            false
        } else {
            true
        }
    });
    if new_def.edges.len() == before {
        return Err("edge_not_found".to_string());
    }

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

    repo.append_version(&harness, &new_version, lock_version)
        .await
        .map_err(map_core_error)?;

    Ok(DeleteEdgeResultDto {
        new_version: new_version_no,
        lock_version: harness.lock_version,
    })
}

/// 指定ハーネスのアクティブ実行（status = running かつ直近300秒以内にハートビートあり）を返す。
/// project_path の末尾コンポーネントが Nipper タブ名になる。
#[tauri::command]
pub async fn get_active_runs(
    state: State<'_, AppState>,
    harness_id: String,
) -> Result<Vec<ActiveRunDto>, String> {
    let runs = state
        .run_repo
        .list_by_harness(&harness_id)
        .await
        .map_err(|e| e.to_string())?;

    let now = chrono::Utc::now();
    let stale_secs = chrono::Duration::seconds(300);

    let active = runs
        .into_iter()
        .filter(|r| r.status == RunStatus::Running)
        .filter(|r| {
            // last_heartbeat_at がなければ updated_at を代用して鮮度確認
            let ts = r.last_heartbeat_at.as_deref().unwrap_or(&r.updated_at);
            chrono::DateTime::parse_from_rfc3339(ts)
                .map(|dt| now.signed_duration_since(dt.with_timezone(&chrono::Utc)) < stale_secs)
                .unwrap_or(false)
        })
        .map(|r| ActiveRunDto {
            run_id: r.id,
            current_cell_id: r.current_cell_id,
            project_path: r.project_path,
        })
        .collect();

    Ok(active)
}

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

#[cfg(test)]
mod tests {
    use super::{add_cell_inner, add_edge_inner, delete_cell_inner, delete_edge_inner, purge_harness_inner, rename_cell_inner, restore_harness_inner, trash_harness_inner};
    use std::sync::Arc;
    use sugo_core::domain::board::BoardDefinition;
    use sugo_core::domain::cell::{Cell, CellStatus};
    use sugo_core::error::CoreError;
    use sugo_core::ports::repository::fake::{FakeIdClock, InMemoryHarnessRepository};
    use sugo_core::ports::repository::HarnessRepository;
    use sugo_core::usecase::create_harness::{create_harness, CreateHarnessInput};
    use sugo_core::usecase::get_status::get_status;

    // ── list_harnesses ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn list_harnesses_returns_all_harnesses() {
        let repo = Arc::new(InMemoryHarnessRepository::new());
        let clock = FakeIdClock::new();
        create_harness(
            repo.as_ref(),
            &clock,
            CreateHarnessInput { name: "alpha".into(), description: None, definition: None },
        )
        .await
        .unwrap();
        create_harness(
            repo.as_ref(),
            &clock,
            CreateHarnessInput { name: "beta".into(), description: None, definition: None },
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
            CreateHarnessInput { name: "h".into(), description: None, definition: Some(def) },
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
            CreateHarnessInput { name: "h".into(), description: None, definition: Some(def) },
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

    // ── add_cell（コマンド本体 add_cell_inner を直接実行）───────────────────

    #[tokio::test]
    async fn add_cell_inner_creates_draft_cell_and_bumps_version() {
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();
        let out = create_harness(
            &repo,
            &clock,
            CreateHarnessInput { name: "h".into(), description: None, definition: None },
        )
        .await
        .unwrap();

        // lock_version は create 後 0
        let res = add_cell_inner(&repo, out.harness_id.clone(), "new".into(), 0)
            .await
            .unwrap();
        assert_eq!(res.new_version, 2);
        assert_eq!(res.lock_version, 1);

        let (h2, bv2) = repo.get(&out.harness_id).await.unwrap().unwrap();
        assert_eq!(h2.current_version, 2);
        assert!(h2.has_draft);
        assert_eq!(h2.lock_version, 1);
        assert_eq!(bv2.definition.cells.len(), 2); // default 1 + added 1
        let added = bv2.definition.cells.last().unwrap();
        assert_eq!(added.name, "new");
        assert_eq!(added.status, CellStatus::Draft);
    }

    #[tokio::test]
    async fn add_cell_inner_unknown_harness_returns_not_found() {
        let repo = InMemoryHarnessRepository::new();
        let err = add_cell_inner(&repo, "nope".into(), "x".into(), 0)
            .await
            .unwrap_err();
        assert!(err.contains("not found"));
    }

    #[tokio::test]
    async fn add_cell_inner_stale_lock_returns_lock_conflict_code() {
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();
        let out = create_harness(
            &repo,
            &clock,
            CreateHarnessInput { name: "h".into(), description: None, definition: None },
        )
        .await
        .unwrap();

        // 実際の head lock は 0。期待 99 で競合させる。
        let err = add_cell_inner(&repo, out.harness_id, "new".into(), 99)
            .await
            .unwrap_err();
        // フロントが分岐する安定コードに一致すること（rename と対称）
        assert_eq!(err, "lock_conflict");
    }

    // ── rename_cell（コマンド本体 rename_cell_inner を直接実行）──────────────

    /// テスト用: name="old"/prompt="p" の単一 active セル c1 を持つハーネスを作る。
    async fn seed_single_cell(repo: &InMemoryHarnessRepository) -> String {
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
        create_harness(
            repo,
            &clock,
            CreateHarnessInput { name: "h".into(), description: None, definition: Some(def) },
        )
        .await
        .unwrap()
        .harness_id
    }

    #[tokio::test]
    async fn rename_cell_inner_changes_name_only_and_bumps_version() {
        let repo = InMemoryHarnessRepository::new();
        let hid = seed_single_cell(&repo).await;

        let res = rename_cell_inner(&repo, hid.clone(), "c1".into(), "new".into(), 0)
            .await
            .unwrap();
        assert_eq!(res.new_version, 2);
        assert_eq!(res.lock_version, 1);

        let (h2, bv2) = repo.get(&hid).await.unwrap().unwrap();
        let c1 = bv2.definition.cells.iter().find(|c| c.id == "c1").unwrap();
        assert_eq!(c1.name, "new");
        assert_eq!(c1.prompt, "p"); // prompt 不変
        assert_eq!(c1.id, "c1"); // id 不変
        assert_eq!(h2.current_version, 2);
        assert_eq!(h2.lock_version, 1);
    }

    #[tokio::test]
    async fn rename_cell_inner_trims_and_rejects_empty_name() {
        let repo = InMemoryHarnessRepository::new();
        let hid = seed_single_cell(&repo).await;

        // 空白のみは empty_name で拒否（バージョンは増えない）
        let err = rename_cell_inner(&repo, hid.clone(), "c1".into(), "   ".into(), 0)
            .await
            .unwrap_err();
        assert_eq!(err, "empty_name");
        let (h, _) = repo.get(&hid).await.unwrap().unwrap();
        assert_eq!(h.current_version, 1);

        // 前後空白はトリムされて保存される
        rename_cell_inner(&repo, hid.clone(), "c1".into(), "  spaced  ".into(), 0)
            .await
            .unwrap();
        let (_, bv) = repo.get(&hid).await.unwrap().unwrap();
        assert_eq!(bv.definition.cells[0].name, "spaced");
    }

    #[tokio::test]
    async fn rename_cell_inner_unknown_cell_returns_not_found() {
        let repo = InMemoryHarnessRepository::new();
        let hid = seed_single_cell(&repo).await;

        let err = rename_cell_inner(&repo, hid, "no-such".into(), "x".into(), 0)
            .await
            .unwrap_err();
        assert!(err.contains("not found"));
    }

    #[tokio::test]
    async fn rename_cell_inner_stale_lock_returns_lock_conflict_code() {
        let repo = InMemoryHarnessRepository::new();
        let hid = seed_single_cell(&repo).await;

        // 実際の head lock は 0。期待 99 で競合させる。
        let err = rename_cell_inner(&repo, hid, "c1".into(), "new".into(), 99)
            .await
            .unwrap_err();
        // フロントが分岐する安定コードに一致すること
        assert_eq!(err, "lock_conflict");
    }

    #[test]
    fn map_core_error_lock_conflict_is_stable_code() {
        let e = CoreError::LockConflict { expected: 1, actual: 2 };
        assert_eq!(super::map_core_error(e), "lock_conflict");
    }

    #[tokio::test]
    async fn delete_cell_inner_removes_draft_cell_and_edges() {
        let repo = InMemoryHarnessRepository::new();
        let hid = seed_single_cell(&repo).await;

        // draft セルを追加してから削除する
        let add_res = add_cell_inner(&repo, hid.clone(), "draft-cell".into(), 0)
            .await
            .unwrap();
        let (_, bv1) = repo.get(&hid).await.unwrap().unwrap();
        let draft_id = bv1
            .definition
            .cells
            .iter()
            .find(|c| c.name == "draft-cell")
            .unwrap()
            .id
            .clone();

        let del_res = delete_cell_inner(&repo, hid.clone(), draft_id.clone(), add_res.lock_version)
            .await
            .unwrap();
        assert_eq!(del_res.new_version, add_res.new_version + 1);

        let (h, bv2) = repo.get(&hid).await.unwrap().unwrap();
        assert!(!bv2.definition.cells.iter().any(|c| c.id == draft_id));
        assert!(!h.has_draft);
    }

    #[tokio::test]
    async fn delete_cell_inner_rejects_non_draft_cell() {
        let repo = InMemoryHarnessRepository::new();
        let hid = seed_single_cell(&repo).await;

        let err = delete_cell_inner(&repo, hid, "c1".into(), 0)
            .await
            .unwrap_err();
        assert_eq!(err, "not_draft");
    }

    // ── add_edge / delete_edge（inner を直接実行）──────────────────────────

    /// c1（active）を持つハーネスに draft セルを1つ足し、(harness_id, draft_id, lock) を返す。
    async fn seed_two_cells(repo: &InMemoryHarnessRepository) -> (String, String, i64) {
        let hid = seed_single_cell(repo).await;
        let add = add_cell_inner(repo, hid.clone(), "second".into(), 0)
            .await
            .unwrap();
        let (_, bv) = repo.get(&hid).await.unwrap().unwrap();
        let draft_id = bv
            .definition
            .cells
            .iter()
            .find(|c| c.name == "second")
            .unwrap()
            .id
            .clone();
        (hid, draft_id, add.lock_version)
    }

    #[tokio::test]
    async fn add_edge_inner_appends_edge_and_bumps_version() {
        let repo = InMemoryHarnessRepository::new();
        let (hid, draft_id, lock) = seed_two_cells(&repo).await;

        let res = add_edge_inner(
            &repo,
            hid.clone(),
            "c1".into(),
            draft_id.clone(),
            "次へ".into(),
            None,
            lock,
        )
        .await
        .unwrap();
        assert_eq!(res.lock_version, lock + 1);

        let (_, bv) = repo.get(&hid).await.unwrap().unwrap();
        assert_eq!(bv.definition.edges.len(), 1);
        let e = &bv.definition.edges[0];
        assert_eq!(e.from, "c1");
        assert_eq!(e.to, draft_id);
        assert_eq!(e.label, "次へ");
        assert!(e.guard.is_none());
    }

    #[tokio::test]
    async fn add_edge_inner_normalizes_guard_and_trims_label() {
        let repo = InMemoryHarnessRepository::new();
        let (hid, draft_id, lock) = seed_two_cells(&repo).await;

        add_edge_inner(
            &repo,
            hid.clone(),
            "c1".into(),
            draft_id.clone(),
            "  進む  ".into(),
            Some("  続ける  ".into()),
            lock,
        )
        .await
        .unwrap();

        let (_, bv) = repo.get(&hid).await.unwrap().unwrap();
        let e = &bv.definition.edges[0];
        assert_eq!(e.label, "進む"); // トリムされる
        assert_eq!(e.guard.as_ref().unwrap().expr, "続ける");
    }

    #[tokio::test]
    async fn add_edge_inner_empty_guard_becomes_none() {
        let repo = InMemoryHarnessRepository::new();
        let (hid, draft_id, lock) = seed_two_cells(&repo).await;

        add_edge_inner(&repo, hid.clone(), "c1".into(), draft_id, "x".into(), Some("   ".into()), lock)
            .await
            .unwrap();
        let (_, bv) = repo.get(&hid).await.unwrap().unwrap();
        assert!(bv.definition.edges[0].guard.is_none());
    }

    #[tokio::test]
    async fn add_edge_inner_rejects_empty_label() {
        let repo = InMemoryHarnessRepository::new();
        let (hid, draft_id, lock) = seed_two_cells(&repo).await;
        let err = add_edge_inner(&repo, hid, "c1".into(), draft_id, "  ".into(), None, lock)
            .await
            .unwrap_err();
        assert_eq!(err, "empty_label");
    }

    #[tokio::test]
    async fn add_edge_inner_rejects_unknown_endpoint() {
        let repo = InMemoryHarnessRepository::new();
        let (hid, _draft_id, lock) = seed_two_cells(&repo).await;
        let err = add_edge_inner(&repo, hid, "c1".into(), "ghost".into(), "x".into(), None, lock)
            .await
            .unwrap_err();
        assert!(err.contains("not found"));
    }

    #[tokio::test]
    async fn add_edge_inner_rejects_duplicate() {
        let repo = InMemoryHarnessRepository::new();
        let (hid, draft_id, lock) = seed_two_cells(&repo).await;
        let r1 = add_edge_inner(&repo, hid.clone(), "c1".into(), draft_id.clone(), "x".into(), None, lock)
            .await
            .unwrap();
        let err = add_edge_inner(&repo, hid, "c1".into(), draft_id, "x".into(), None, r1.lock_version)
            .await
            .unwrap_err();
        assert_eq!(err, "duplicate_edge");
    }

    #[tokio::test]
    async fn add_edge_inner_stale_lock_returns_lock_conflict_code() {
        let repo = InMemoryHarnessRepository::new();
        let (hid, draft_id, _lock) = seed_two_cells(&repo).await;
        let err = add_edge_inner(&repo, hid, "c1".into(), draft_id, "x".into(), None, 99)
            .await
            .unwrap_err();
        assert_eq!(err, "lock_conflict");
    }

    #[tokio::test]
    async fn delete_edge_inner_removes_matching_edge() {
        let repo = InMemoryHarnessRepository::new();
        let (hid, draft_id, lock) = seed_two_cells(&repo).await;
        let add = add_edge_inner(&repo, hid.clone(), "c1".into(), draft_id.clone(), "x".into(), None, lock)
            .await
            .unwrap();

        let res = delete_edge_inner(&repo, hid.clone(), "c1".into(), draft_id, "x".into(), add.lock_version)
            .await
            .unwrap();
        assert_eq!(res.lock_version, add.lock_version + 1);
        let (_, bv) = repo.get(&hid).await.unwrap().unwrap();
        assert!(bv.definition.edges.is_empty());
    }

    #[tokio::test]
    async fn delete_edge_inner_unknown_returns_not_found() {
        let repo = InMemoryHarnessRepository::new();
        let (hid, draft_id, lock) = seed_two_cells(&repo).await;
        let err = delete_edge_inner(&repo, hid, "c1".into(), draft_id, "nope".into(), lock)
            .await
            .unwrap_err();
        assert_eq!(err, "edge_not_found");
    }

    // ── trash_harness（inner）────────────────────────────────────────────────

    async fn seed_harness(repo: &InMemoryHarnessRepository) -> String {
        let clock = FakeIdClock::new();
        create_harness(
            repo,
            &clock,
            CreateHarnessInput { name: "h".into(), description: None, definition: None },
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
}
