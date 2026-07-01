use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct HarnessSummaryDto {
    pub harness_id: String,
    pub name: String,
    pub current_version: i64,
    pub has_draft: bool,
}

#[derive(Debug, Serialize)]
pub struct CellDto {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub status: String,
    pub terminal: bool,
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

#[derive(Debug, Serialize)]
pub struct TrashItemDto {
    pub harness_id: String,
    pub name: String,
    pub deleted_at: String,
    pub remaining_days: i64,
}
