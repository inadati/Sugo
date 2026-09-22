//! Output port for cell display-position persistence.

use crate::domain::cell_position::CellPosition;
use crate::error::CoreError;
use async_trait::async_trait;

#[async_trait]
pub trait CellPositionRepository: Send + Sync {
    /// ハーネスの全セル座標を返す。未登録なら空 Vec。
    async fn list(&self, harness_id: &str) -> Result<Vec<CellPosition>, CoreError>;

    /// ハーネスの座標を与えられた集合で全置換する。
    ///
    /// 削除・挿入を単一トランザクションで行うため、盤面から消えたセルの
    /// 座標が残留しない。GUI の保存はこちらを使う。
    async fn replace_all(
        &self,
        harness_id: &str,
        positions: &[CellPosition],
    ) -> Result<(), CoreError>;

    /// 指定されたセルの座標だけを上書きする。他セルの座標は保持する。
    async fn upsert(
        &self,
        harness_id: &str,
        positions: &[CellPosition],
    ) -> Result<(), CoreError>;

    /// ハーネスの座標を全削除する。次の描画で自動レイアウトが走る。
    async fn clear(&self, harness_id: &str) -> Result<(), CoreError>;
}
