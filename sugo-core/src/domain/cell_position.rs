//! セルの表示座標。盤面の意味論ではなく見た目の情報であり、
//! `BoardDefinition` とは別に永続化される。

/// 1セル分の表示座標。`x`/`y` はセル中心の座標。
#[derive(Debug, Clone, PartialEq)]
pub struct CellPosition {
    pub cell_id: String,
    pub x: f64,
    pub y: f64,
}
