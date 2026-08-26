//! ハーネスを分類するユーザー命名フォルダ。
//!
//! フォルダは盤面定義ではなくメタデータであり、board version を起こさない。
//! `parent_id` は将来の入れ子対応のために予約された列で、フラット運用中は
//! 常に `None` である。

/// ユーザーが名前を付けたハーネス分類フォルダ。
#[derive(Debug, Clone, PartialEq)]
pub struct Folder {
    /// 一意なフォルダ ID。
    pub id: String,
    /// 表示名（trim 済み、1〜64 文字）。
    pub name: String,
    /// 親フォルダ ID。フラット運用中は常に `None`（入れ子は未実装）。
    pub parent_id: Option<String>,
    /// サイドバーでの並び順。作成順に採番する。
    pub sort_order: i64,
    /// 作成時刻（ISO 8601）。
    pub created_at: String,
    /// 更新時刻（ISO 8601）。
    pub updated_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_is_flat_by_default() {
        let f = Folder {
            id: "f1".into(),
            name: "開発".into(),
            parent_id: None,
            sort_order: 0,
            created_at: "2026-08-26T00:00:00+09:00".into(),
            updated_at: "2026-08-26T00:00:00+09:00".into(),
        };
        assert!(f.parent_id.is_none());
        assert_eq!(f.name, "開発");
    }
}
