import { describe, it, expect, vi, beforeEach } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

import { loadPositions, savePositions, clearPositions } from "./positions";

beforeEach(() => invoke.mockReset());

describe("loadPositions", () => {
  it("コマンドの配列を id をキーにしたマップへ変換する", async () => {
    invoke.mockResolvedValue([
      { cell_id: "c1", x: 1, y: 2 },
      { cell_id: "c2", x: 3, y: 4 },
    ]);
    expect(await loadPositions("h1")).toEqual({ c1: { x: 1, y: 2 }, c2: { x: 3, y: 4 } });
    expect(invoke).toHaveBeenCalledWith("get_cell_positions", { harnessId: "h1" });
  });

  it("異常な座標（NaN・非有限・極端に大きい値）を捨てる", async () => {
    invoke.mockResolvedValue([
      { cell_id: "ok", x: 1, y: 2 },
      { cell_id: "nan", x: Number.NaN, y: 0 },
      { cell_id: "inf", x: 0, y: Number.POSITIVE_INFINITY },
      { cell_id: "huge", x: 999999, y: 0 },
    ]);
    expect(await loadPositions("h1")).toEqual({ ok: { x: 1, y: 2 } });
  });
});

describe("savePositions", () => {
  it("マップを配列へ変換してコマンドへ渡す", async () => {
    invoke.mockResolvedValue(undefined);
    await savePositions("h1", { c1: { x: 1, y: 2 } });
    expect(invoke).toHaveBeenCalledWith("save_cell_positions", {
      harnessId: "h1",
      positions: [{ cell_id: "c1", x: 1, y: 2 }],
    });
  });
});

describe("clearPositions", () => {
  it("コマンドを呼ぶ", async () => {
    invoke.mockResolvedValue(undefined);
    await clearPositions("h1");
    expect(invoke).toHaveBeenCalledWith("clear_cell_positions", { harnessId: "h1" });
  });
});
