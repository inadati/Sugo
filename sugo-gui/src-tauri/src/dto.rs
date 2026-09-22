use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct HarnessSummaryDto {
    pub harness_id: String,
    pub name: String,
    pub current_version: i64,
    pub has_draft: bool,
    pub folder_id: Option<String>,
    pub folder_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FolderDto {
    pub folder_id: String,
    pub name: String,
    pub harness_count: i64,
}

#[derive(Debug, Serialize)]
pub struct DeleteFolderResultDto {
    pub name: String,
    pub moved_to_uncategorized: i64,
}

#[derive(Debug, Serialize)]
pub struct CellDto {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub status: String,
    pub terminal: bool,
    pub memo: String,
}

#[derive(Debug, Serialize)]
pub struct EdgeDto {
    pub from: String,
    pub to: String,
    pub label: String,
    pub guard: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DraftCellDto {
    pub cell_id: String,
    pub name: String,
    pub memo: String,
}

#[derive(Debug, Serialize)]
pub struct HarnessDetailDto {
    pub harness_id: String,
    pub name: String,
    pub current_version: i64,
    pub lock_version: i64,
    pub has_draft: bool,
    /// 権威である START マスの id（definition.start）。フロントは cells[0] で
    /// 代用せず、必ずこの値で START を判定する。
    pub start_cell_id: String,
    pub cells: Vec<CellDto>,
    pub edges: Vec<EdgeDto>,
    pub draft_diff: Vec<DraftCellDto>,
}

#[derive(Debug, Serialize)]
pub struct CreateHarnessResultDto {
    pub harness_id: String,
}

#[derive(Debug, Serialize)]
pub struct AddCellResultDto {
    pub new_version: i64,
    pub lock_version: i64,
}

#[derive(Debug, Serialize)]
pub struct RenameCellResultDto {
    pub new_version: i64,
    pub lock_version: i64,
}

#[derive(Debug, Serialize)]
pub struct DeleteCellResultDto {
    pub new_version: i64,
    pub lock_version: i64,
}

#[derive(Debug, Serialize)]
pub struct AddEdgeResultDto {
    pub new_version: i64,
    pub lock_version: i64,
}

#[derive(Debug, Serialize)]
pub struct DeleteEdgeResultDto {
    pub new_version: i64,
    pub lock_version: i64,
}

#[derive(Debug, Serialize)]
pub struct UpdateEdgeResultDto {
    pub new_version: i64,
    pub lock_version: i64,
}

#[derive(Debug, Serialize)]
pub struct ActiveRunDto {
    pub run_id: String,
    pub current_cell_id: String,
    pub project_path: Option<String>,
}

/// `stop_run` の結果。`was_in_flight` が false のときは、そのランが既に
/// 終了していて何も変更されなかったことを意味する。
#[derive(Debug, Serialize)]
pub struct StopRunResultDto {
    pub was_in_flight: bool,
    pub stopped_at_cell_id: String,
}

#[derive(Debug, Serialize)]
pub struct TrashItemDto {
    pub harness_id: String,
    pub name: String,
    pub deleted_at: String,
    pub remaining_days: i64,
}

/// セルの表示座標。x/y はセル中心の座標。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CellPositionDto {
    pub cell_id: String,
    pub x: f64,
    pub y: f64,
}
