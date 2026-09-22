import { test, expect } from "@playwright/test";

/**
 * folders.spec.ts と同じ手法で `window.__TAURI_INTERNALS__` をスタブし、
 * HarnessView / BoardGraph はビルド済みの本物をそのまま動かす。
 *
 * 蛇行レイアウトの結果は cytoscape の canvas に描画されるだけで DOM に
 * ノード座標が残らないため、テスト専用フックを本番コードに足す代わりに
 * 「レイアウト結果が `save_cell_positions` に保存される」という実際の
 * データフローを観測点にする。`get_cell_positions` を空配列にして新規
 * ハーネス相当の状態を作り、BoardGraph が自動レイアウト
 * （lib/layout.ts の ELK 蛇行レイアウト）を実行した結果を
 * `save_cell_positions` の呼び出し引数として記録し、それを検証する。
 */
function installTauriStub() {
  interface Cell {
    id: string;
    name: string;
    prompt: string;
    status: string;
    terminal: boolean;
    memo: string;
  }
  interface Edge {
    from: string;
    to: string;
    label: string;
    guard: string | null;
  }
  interface SavedPosition {
    cell_id: string;
    x: number;
    y: number;
  }

  // 一本鎖: c1 -> c2 -> ... -> c6（6マス以上）。c1 が START、c6 が END。
  const cellIds = ["c1", "c2", "c3", "c4", "c5", "c6"];
  const cells: Cell[] = cellIds.map((id, i) => ({
    id,
    name: `セル${i + 1}`,
    prompt: `プロンプト${i + 1}`,
    status: "active",
    terminal: i === cellIds.length - 1,
    memo: "",
  }));
  const edges: Edge[] = [];
  for (let i = 0; i < cellIds.length - 1; i++) {
    edges.push({ from: cellIds[i], to: cellIds[i + 1], label: "next", guard: null });
  }

  const w = window as unknown as {
    __TAURI_INTERNALS__?: { invoke: (cmd: string, args?: Record<string, unknown>) => Promise<unknown> };
    __savedPositionCalls?: SavedPosition[][];
  };
  // save_cell_positions に渡された payload を呼び出しごとに蓄積する。
  // テストは最後（＝自動レイアウト後）の payload を読む。
  w.__savedPositionCalls = [];

  w.__TAURI_INTERNALS__ = {
    invoke: async (cmd: string, args: Record<string, unknown> = {}) => {
      switch (cmd) {
        case "get_harness":
          return {
            harness_id: "h1",
            name: "chain-harness",
            current_version: 1,
            lock_version: 1,
            has_draft: false,
            start_cell_id: "c1",
            cells,
            edges,
            draft_diff: [],
          };
        case "get_active_runs":
          return [];
        case "get_cell_positions":
          // 空配列を返すことで「保存済み配置が無い＝新規ハーネス」を再現し、
          // BoardGraph に自動レイアウト（relayoutAll 経由）を実行させる。
          return [];
        case "save_cell_positions": {
          const positions = (args.positions as SavedPosition[] | undefined) ?? [];
          w.__savedPositionCalls!.push(positions);
          return null;
        }
        default:
          return null;
      }
    },
  };
}

test("新規ハーネスの盤面は横一列にならない", async ({ page }) => {
  await page.addInitScript(installTauriStub);
  await page.goto("/harness/h1");

  // BoardGraph 初期化 → 自動レイアウト（relayoutAll）→ save_cell_positions
  // という一連の非同期処理が完了するのを待つ。
  await expect
    .poll(async () => page.evaluate(() => (window as unknown as { __savedPositionCalls: unknown[][] }).__savedPositionCalls.length))
    .toBeGreaterThan(0);

  const lastCall = await page.evaluate(() => {
    const calls = (window as unknown as { __savedPositionCalls: { cell_id: string; x: number; y: number }[][] })
      .__savedPositionCalls;
    return calls[calls.length - 1];
  });

  expect(lastCall.length).toBe(6);

  const ys = lastCall.map((p) => Math.round(p.y));

  // 折り返されていれば y が複数種類になる。横一列（この計画で直したバグ）なら1種類しかない。
  expect(new Set(ys).size).toBeGreaterThan(1);
});
