// 蛇行（ジグザグ）レイアウト。
//
// ELK には層への割り当てとノード順序の決定（交差最小化）だけを任せ、
// 行への折り返しは自前で行う。ELK 自身の wrapping 機能を使わない理由は、
// 出力に層の情報が含まれず、分岐のあるグラフでは座標から行の境界を
// 復元できないため。行が確定できなければ「奇数行を反転する」蛇行処理が
// 成立しない。設計の詳細は
// docs/superpowers/specs/2026-09-22-cell-layout-design.md を参照。
//
// このモジュールは cytoscape・Tauri・DB のいずれにも依存しない純粋な計算で、
// 同じ入力からは常に同じ座標を返す。
import ELK from "elkjs/lib/elk.bundled.js";
import type { PositionMap } from "./positions";

export type LayoutNode = { id: string; width: number; height: number };
export type LayoutEdge = { from: string; to: string };
/** ELK 由来の配置結果。x/y はノードの左上。 */
export type PlacedNode = { id: string; x: number; y: number; width: number; height: number };
/** positions はセル中心の座標。width/height は外接矩形の大きさ。 */
export type ArrangeResult = { positions: PositionMap; width: number; height: number };

/** 目標の縦横比。横長すぎると文字が潰れ、縦長すぎると全体が見渡せない。 */
const TARGET_ASPECT = 1.6;
/** 行と行の縦の余白。 */
const ROW_GAP = 90;

const elk = new ELK();

/** ELK layered を折り返しなしで実行し、層割り当てとノード順序を得る。 */
async function runElk(nodes: LayoutNode[], edges: LayoutEdge[]): Promise<PlacedNode[]> {
  const graph = await elk.layout({
    id: "root",
    layoutOptions: {
      "elk.algorithm": "layered",
      "elk.direction": "RIGHT",
      // 折り返しは自前で行うため OFF。理由はモジュール冒頭のコメント参照。
      "elk.layered.wrapping.strategy": "OFF",
      "elk.spacing.nodeNode": "40",
      "elk.layered.spacing.nodeNodeBetweenLayers": "80",
    },
    children: nodes.map((n) => ({ id: n.id, width: n.width, height: n.height })),
    edges: edges.map((e, i) => ({ id: `e${i}`, sources: [e.from], targets: [e.to] })),
  });
  return (graph.children ?? []).map((c) => {
    if (c.x == null || c.y == null || c.width == null || c.height == null) {
      throw new Error(`ELK returned a child without full coordinates: ${c.id}`);
    }
    return { id: c.id, x: c.x, y: c.y, width: c.width, height: c.height };
  });
}

/**
 * 同じ x のノードを1つの層とみなし、x 昇順の層配列を返す。
 *
 * direction=RIGHT の layered 出力では同一層のノードが同一の x に揃うため、
 * x が層の識別子として使える。
 *
 * この判定は ELK が層を左揃えで出力すること、かつ Sugo の全セルが幅固定
 * （150px、BoardGraph.vue の STYLES で設定）であることに依存している。
 * 将来セル幅を可変にする場合はこの関数の前提が崩れるので注意すること。
 */
export function groupIntoLayers(placed: PlacedNode[]): PlacedNode[][] {
  const byX = new Map<number, PlacedNode[]>();
  for (const n of placed) {
    const k = Math.round(n.x);
    if (!byX.has(k)) byX.set(k, []);
    byX.get(k)!.push(n);
  }
  return [...byX.keys()]
    .sort((a, b) => a - b)
    .map((k) => byX.get(k)!.slice().sort((a, b) => a.y - b.y));
}

/**
 * 層配列を `layersPerRow` 層ずつの行に切り、行を縦に積む。
 *
 * `serpentine` が true のとき、奇数行は行内で x を左右反転する。これにより
 * 行の進行方向が交互になり、行末から次の行頭へのエッジが画面を横断する
 * 長い線ではなく真下への短い線になる。
 */
export function arrangeRows(
  layers: PlacedNode[][],
  layersPerRow: number,
  serpentine = true,
): ArrangeResult {
  const positions: PositionMap = {};
  let rowTop = 0;
  let maxWidth = 0;

  for (let r = 0; r * layersPerRow < layers.length; r++) {
    const flat = layers.slice(r * layersPerRow, (r + 1) * layersPerRow).flat();
    if (flat.length === 0) continue;

    const minX = Math.min(...flat.map((n) => n.x));
    const rowWidth = Math.max(...flat.map((n) => n.x + n.width)) - minX;
    const minY = Math.min(...flat.map((n) => n.y));
    const rowHeight = Math.max(...flat.map((n) => n.y + n.height)) - minY;

    for (const n of flat) {
      const localX = n.x - minX;
      const left = serpentine && r % 2 === 1 ? rowWidth - (localX + n.width) : localX;
      // 左上基準から中心基準へ変換する（cytoscape の node.position() に合わせる）
      positions[n.id] = {
        x: left + n.width / 2,
        y: rowTop + (n.y - minY) + n.height / 2,
      };
    }

    maxWidth = Math.max(maxWidth, rowWidth);
    rowTop += rowHeight + ROW_GAP;
  }

  return { positions, width: maxWidth, height: Math.max(0, rowTop - ROW_GAP) };
}

/** 縦横比が目標に最も近くなる「1行あたりの層数」を全探索で選ぶ。 */
export function chooseLayersPerRow(
  layers: PlacedNode[][],
  targetAspect = TARGET_ASPECT,
): number {
  let best = Math.max(1, layers.length);
  let bestDiff = Number.POSITIVE_INFINITY;
  for (let k = 1; k <= layers.length; k++) {
    const { width, height } = arrangeRows(layers, k);
    if (height === 0) continue;
    const diff = Math.abs(width / height - targetAspect);
    if (diff < bestDiff) {
      bestDiff = diff;
      best = k;
    }
  }
  return best;
}

/** 盤面から蛇行レイアウトの座標（セル中心）を計算する。 */
export async function computeLayout(
  nodes: LayoutNode[],
  edges: LayoutEdge[],
  targetAspect = TARGET_ASPECT,
): Promise<PositionMap> {
  if (nodes.length === 0) return {};
  const placed = await runElk(nodes, edges);
  const layers = groupIntoLayers(placed);
  return arrangeRows(layers, chooseLayersPerRow(layers, targetAspect)).positions;
}
