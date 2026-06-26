use super::board::BoardDefinition;

#[derive(Debug, Clone, PartialEq)]
pub struct Harness {
    pub id: String,
    pub name: String,
    pub current_version: i64,
    pub has_draft: bool,
    pub lock_version: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BoardVersion {
    pub id: String,
    pub harness_id: String,
    pub version_no: i64,
    pub definition: BoardDefinition,
    pub content_hash: String,
    pub created_at: String,
}
