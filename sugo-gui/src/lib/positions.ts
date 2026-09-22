// セル表示座標の永続化。Tauri コマンド越しに SQLite の cell_positions を読み書きする。
//
// 以前は localStorage に保存していたが、sugo-mcp（別プロセスの Rust サーバー）が
// 座標を読み書きできるよう DB へ移した。localStorage からの移行は行わない
// （座標は盤面から再計算でき、そこに残っているのは解消対象の横一列配置であるため）。
import { invoke } from "@tauri-apps/api/core";

export type PositionMap = Record<string, { x: number; y: number }>;

type CellPositionDto = { cell_id: string; x: number; y: number };

// 過去の不具合で異常な座標が保存されると cy.fit() が極端にズームアウトし、
// 他のノードが実質見えなくなる。そのセルだけ「未保存」扱いにして自動配置へ
// 回すことで自己修復する。
const MAX_SANE_COORD = 20000;

function isSaneCoord(x: number, y: number): boolean {
  return (
    Number.isFinite(x) && Math.abs(x) <= MAX_SANE_COORD &&
    Number.isFinite(y) && Math.abs(y) <= MAX_SANE_COORD
  );
}

export async function loadPositions(harnessId: string): Promise<PositionMap> {
  const rows = await invoke<CellPositionDto[]>("get_cell_positions", { harnessId });
  const out: PositionMap = {};
  for (const r of rows) {
    if (isSaneCoord(r.x, r.y)) out[r.cell_id] = { x: r.x, y: r.y };
  }
  return out;
}

export async function savePositions(harnessId: string, positions: PositionMap): Promise<void> {
  const rows: CellPositionDto[] = Object.entries(positions).map(([cell_id, p]) => ({
    cell_id,
    x: p.x,
    y: p.y,
  }));
  await invoke("save_cell_positions", { harnessId, positions: rows });
}

export async function clearPositions(harnessId: string): Promise<void> {
  await invoke("clear_cell_positions", { harnessId });
}
