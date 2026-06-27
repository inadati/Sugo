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
    pub cells: Vec<CellDto>,
    pub edges: Vec<EdgeDto>,
    pub draft_diff: Vec<DraftCellDto>,
}

#[derive(Debug, Serialize)]
pub struct AddCellResultDto {
    pub new_version: i64,
    pub lock_version: i64,
}
