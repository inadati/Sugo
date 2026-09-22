import { describe, it, expect } from "vitest";
import {
  computeLayout,
  groupIntoLayers,
  arrangeRows,
  type LayoutNode,
  type LayoutEdge,
  type PlacedNode,
} from "./layout";
import type { PositionMap } from "./positions";

const NODE_W = 150;
const NODE_H = 70;

/** n マスの一本鎖を作る。 */
function chain(n: number): { nodes: LayoutNode[]; edges: LayoutEdge[] } {
  const nodes = Array.from({ length: n }, (_, i) => ({
    id: `c${i}`,
    width: NODE_W,
    height: NODE_H,
  }));
  const edges = Array.from({ length: n - 1 }, (_, i) => ({ from: `c${i}`, to: `c${i + 1}` }));
  return { nodes, edges };
}

/** 2線分が交差するかを判定する（端点の共有は交差とみなさない）。 */
function segmentsCross(
  a1: { x: number; y: number }, a2: { x: number; y: number },
  b1: { x: number; y: number }, b2: { x: number; y: number },
): boolean {
  const d = (p: { x: number; y: number }, q: { x: number; y: number }, r: { x: number; y: number }) =>
    (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
  const d1 = d(b1, b2, a1), d2 = d(b1, b2, a2);
  const d3 = d(a1, a2, b1), d4 = d(a1, a2, b2);
  return ((d1 > 0 && d2 < 0) || (d1 < 0 && d2 > 0)) && ((d3 > 0 && d4 < 0) || (d3 < 0 && d4 > 0));
}

/** 座標マップとエッジから、直線で描いた場合の交差数を数える。 */
function countCrossings(positions: PositionMap, edges: LayoutEdge[]): number {
  const segs = edges
    .map((e) => ({ a: positions[e.from], b: positions[e.to] }))
    .filter((s) => s.a != null && s.b != null);
  let n = 0;
  for (let i = 0; i < segs.length; i++) {
    for (let j = i + 1; j < segs.length; j++) {
      if (segmentsCross(segs[i].a, segs[i].b, segs[j].a, segs[j].b)) n++;
    }
  }
  return n;
}

/** 座標マップの外接矩形の縦横比。 */
function aspect(positions: PositionMap): number {
  const xs = Object.values(positions).map((p) => p.x);
  const ys = Object.values(positions).map((p) => p.y);
  const w = Math.max(...xs) - Math.min(...xs) + NODE_W;
  const h = Math.max(...ys) - Math.min(...ys) + NODE_H;
  return w / h;
}

describe("computeLayout", () => {
  it("空の盤面では空のマップを返す", async () => {
    expect(await computeLayout([], [])).toEqual({});
  });

  it("決定的である（同じ入力から同じ座標）", async () => {
    const { nodes, edges } = chain(24);
    const a = await computeLayout(nodes, edges);
    const b = await computeLayout(nodes, edges);
    expect(a).toEqual(b);
  });

  it("一本鎖24マスを複数行に折り返す", async () => {
    const { nodes, edges } = chain(24);
    const positions = await computeLayout(nodes, edges);
    const rows = new Set(Object.values(positions).map((p) => Math.round(p.y)));
    expect(rows.size).toBeGreaterThan(1);
  });

  it("一本鎖24マスの縦横比を 3.0 以下に収める", async () => {
    const { nodes, edges } = chain(24);
    expect(aspect(await computeLayout(nodes, edges))).toBeLessThanOrEqual(3.0);
  });

  it("一本鎖24マスでは交差が発生しない", async () => {
    const { nodes, edges } = chain(24);
    expect(countCrossings(await computeLayout(nodes, edges), edges)).toBe(0);
  });

  it("一本鎖では行ごとに進行方向が反転する（蛇行）", async () => {
    const { nodes, edges } = chain(24);
    const positions = await computeLayout(nodes, edges);
    // y でグループ化し、各行を x 昇順に並べたとき、
    // 偶数行は添字が昇順、奇数行は降順になる。
    const byRow = new Map<number, string[]>();
    for (const [id, p] of Object.entries(positions)) {
      const k = Math.round(p.y);
      if (!byRow.has(k)) byRow.set(k, []);
      byRow.get(k)!.push(id);
    }
    const rows = [...byRow.keys()].sort((a, b) => a - b);
    expect(rows.length).toBeGreaterThan(1);
    rows.forEach((rowY, r) => {
      const idx = byRow
        .get(rowY)!
        .sort((a, b) => positions[a].x - positions[b].x)
        .map((id) => Number(id.slice(1)));
      const ascending = idx.every((v, i) => i === 0 || v > idx[i - 1]);
      const descending = idx.every((v, i) => i === 0 || v < idx[i - 1]);
      expect(r % 2 === 0 ? ascending : descending).toBe(true);
    });
  });
});

describe("arrangeRows", () => {
  /** 層番号と層内位置から PlacedNode を作る（ELK 左上基準）。 */
  function layer(ids: string[], layerIndex: number): PlacedNode[] {
    return ids.map((id, i) => ({
      id,
      x: layerIndex * 230,
      y: i * 100,
      width: NODE_W,
      height: NODE_H,
    }));
  }

  it("蛇行ありは同方向折り返しより交差が増えない", () => {
    // 8層の鎖 + 戻りエッジ（c7→c0）を4層/行で2行に折り返す。
    // この形状は同方向折り返し（naive）では交差が発生し、蛇行では発生しない
    // ことを事前に検証済み（naive=1, snake=0）。
    const layers = [
      layer(["c0"], 0), layer(["c1"], 1), layer(["c2"], 2), layer(["c3"], 3),
      layer(["c4"], 4), layer(["c5"], 5), layer(["c6"], 6), layer(["c7"], 7),
    ];
    const edges: LayoutEdge[] = [
      { from: "c0", to: "c1" }, { from: "c1", to: "c2" }, { from: "c2", to: "c3" },
      { from: "c3", to: "c4" }, { from: "c4", to: "c5" }, { from: "c5", to: "c6" },
      { from: "c6", to: "c7" }, { from: "c7", to: "c0" },
    ];
    const naive = countCrossings(arrangeRows(layers, 4, false).positions, edges);
    const snake = countCrossings(arrangeRows(layers, 4, true).positions, edges);
    expect(naive).toBeGreaterThan(0);
    expect(snake).toBeLessThanOrEqual(naive);
  });

  it("中心座標を返す（左上ではない）", () => {
    const layers = [layer(["c0"], 0)];
    const { positions } = arrangeRows(layers, 1, true);
    // 行内で正規化され左上は (0,0)。中心はその半分ずれる。
    expect(positions.c0).toEqual({ x: NODE_W / 2, y: NODE_H / 2 });
  });
});

describe("groupIntoLayers", () => {
  it("同じ x のノードを1つの層にまとめ、x 昇順で返す", () => {
    const placed: PlacedNode[] = [
      { id: "b", x: 230, y: 0, width: NODE_W, height: NODE_H },
      { id: "a1", x: 0, y: 0, width: NODE_W, height: NODE_H },
      { id: "a2", x: 0, y: 100, width: NODE_W, height: NODE_H },
    ];
    const layers = groupIntoLayers(placed);
    expect(layers.map((l) => l.map((n) => n.id))).toEqual([["a1", "a2"], ["b"]]);
  });
});
