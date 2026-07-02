<template>
  <div ref="container" tabindex="0" class="relative w-full h-full bg-gray-50 rounded border outline-none">
    <!-- ピンオーバーレイ（pointer-events-none でグラフ操作を透過） -->
    <div class="absolute inset-0 pointer-events-none" style="z-index: 10; overflow: hidden;">
      <div
        v-for="m in renderedMarkers"
        :key="m.run_id"
        class="absolute flex flex-col items-center"
        :style="{ left: m.x + 'px', top: m.y + 'px', transform: 'translate(-50%, -100%)' }"
      >
        <div class="text-xs font-semibold text-white bg-orange-500 rounded-full px-2 py-0.5 shadow-md whitespace-nowrap">
          {{ m.tabName }}
        </div>
        <div style="
          width: 0; height: 0;
          border-left: 5px solid transparent;
          border-right: 5px solid transparent;
          border-top: 7px solid #f97316;
        " />
      </div>
    </div>

    <!-- 接続ハンドル（ホバー中ノードの右縁。ここからドラッグして線を引く） -->
    <div
      v-if="handle"
      data-testid="connect-handle"
      class="absolute z-20"
      :style="{ left: handle.x + 'px', top: handle.y + 'px', transform: 'translate(-50%, -50%)' }"
      @mousedown.stop.prevent="startConnect"
      title="ドラッグして接続"
    >
      <div class="w-4 h-4 rounded-full bg-orange-500 border-2 border-white shadow cursor-crosshair hover:scale-125 transition-transform" />
    </div>

    <!-- ノード名インライン編集 -->
    <NodeNameEditor
      v-if="nodeEditor"
      :initial-name="nodeEditor.name"
      :x="nodeEditor.x"
      :y="nodeEditor.y"
      :width="nodeEditor.width"
      @commit="onNodeNameCommit"
      @cancel="nodeEditor = null"
    />

    <!-- 全体表示: レイアウト崩れ等でマスが画面外に見えなくなった場合の保険 -->
    <button
      data-testid="fit-view"
      class="absolute top-2 right-2 z-20 px-2 py-1 text-xs bg-white border border-gray-300 rounded shadow hover:bg-gray-50"
      title="全マスが収まるように表示し直す"
      @click="fitView"
    >全体表示</button>
  </div>
</template>

<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted } from "vue";
import cytoscape from "cytoscape";
import cytoscapeDagre from "cytoscape-dagre";
// @ts-expect-error cytoscape-edgehandles は型を同梱せず、JS 実体が ambient 宣言より優先されるため
import edgehandles from "cytoscape-edgehandles";
import NodeNameEditor from "./NodeNameEditor.vue";

cytoscape.use(cytoscapeDagre as cytoscape.Ext);
cytoscape.use(edgehandles as cytoscape.Ext);

interface CellData {
  id: string;
  name: string;
  status: string;
  terminal: boolean;
}
interface EdgeData {
  from: string;
  to: string;
  label: string;
  guard: string | null;
}
interface ActiveRunData {
  run_id: string;
  current_cell_id: string;
  project_path: string | null;
}
interface RenderedMarker {
  run_id: string;
  tabName: string;
  x: number;
  y: number;
}

type PositionMap = Record<string, { x: number; y: number }>;

const props = defineProps<{
  harnessId: string;
  cells: CellData[];
  edges: EdgeData[];
  startCellId: string;
  activeRuns?: ActiveRunData[];
}>();

const emit = defineEmits<{
  select: [cellId: string];
  connect: [payload: { from: string; to: string; x: number; y: number }];
  edgeEdit: [payload: { from: string; to: string; label: string; guard: string | null; x: number; y: number }];
  edgeDelete: [payload: { from: string; to: string; label: string; guard: string | null }];
  nodeDelete: [cellId: string];
  nodeRename: [payload: { cellId: string; name: string }];
}>();

const container = ref<HTMLElement | null>(null);
let cy: cytoscape.Core | null = null;
let eh: cytoscape.EdgehandlesInstance | null = null;
const renderedMarkers = ref<RenderedMarker[]>([]);

// 現在選択中の要素（Delete キー対象）
const selected = ref<{ type: "node" | "edge"; id: string } | null>(null);
// 接続ハンドルのオーバーレイ位置
const handle = ref<{ x: number; y: number; nodeId: string } | null>(null);
// ノード名インライン編集の状態
const nodeEditor = ref<{ cellId: string; name: string; x: number; y: number; width: number } | null>(null);

// ── レイアウト永続化 ──────────────────────────────────────────────────

function lsKey() {
  return `sugo:layout:${props.harnessId}`;
}

function savePositions() {
  if (!cy) return;
  const pos: PositionMap = {};
  cy.nodes().forEach((n) => { pos[n.id()] = { ...n.position() }; });
  localStorage.setItem(lsKey(), JSON.stringify(pos));
}

// 過去のバグ等でlocalStorageに異常な座標（NaN・非有限値・極端に大きい値）が
// 保存されていると、cy.fit()がそれを含めようとして異常にズームアウトし、
// 離れた場所にある他のノードが実質見えなくなる（#はみ出し対応時に発覚）。
// そのノードだけ「未保存」扱いにして自動配置に回すことで自己修復する。
const MAX_SANE_COORD = 20000;

function isSaneCoord(p: unknown): p is { x: number; y: number } {
  if (!p || typeof p !== "object") return false;
  const { x, y } = p as { x?: unknown; y?: unknown };
  return (
    typeof x === "number" && Number.isFinite(x) && Math.abs(x) <= MAX_SANE_COORD &&
    typeof y === "number" && Number.isFinite(y) && Math.abs(y) <= MAX_SANE_COORD
  );
}

function loadPositions(): PositionMap | null {
  try {
    const raw = localStorage.getItem(lsKey());
    if (!raw) return null;
    const parsed = JSON.parse(raw) as PositionMap;
    const sanitized: PositionMap = {};
    for (const [id, p] of Object.entries(parsed)) {
      if (isSaneCoord(p)) sanitized[id] = p;
    }
    return sanitized;
  } catch {
    return null;
  }
}

function applyPositions(positions: PositionMap) {
  cy?.nodes().forEach((n) => {
    const p = positions[n.id()];
    if (p) n.position(p);
  });
}

// ── グラフ要素構築 ────────────────────────────────────────────────────

// cytoscape の text-wrap: "wrap" は単語（空白）境界でしか折り返さないため、
// スペースを含まない日本語のようなテキストは折り返されずノード幅からはみ出す。
// 実際のテキスト幅を測って自前で改行（\n）を挿入することで確実に折り返す。
const NODE_FONT = "12px sans-serif";
const NODE_TEXT_MAX_WIDTH = 140;
let measureCtx: CanvasRenderingContext2D | null = null;

function wrapText(text: string, maxWidthPx: number): string {
  if (!measureCtx) {
    measureCtx = document.createElement("canvas").getContext("2d");
  }
  if (!measureCtx) return text;
  measureCtx.font = NODE_FONT;
  const lines: string[] = [];
  let currentLine = "";
  for (const ch of text) {
    const testLine = currentLine + ch;
    if (measureCtx.measureText(testLine).width > maxWidthPx && currentLine.length > 0) {
      lines.push(currentLine);
      currentLine = ch;
    } else {
      currentLine = testLine;
    }
  }
  if (currentLine) lines.push(currentLine);
  return lines.join("\n");
}

function buildLabel(c: CellData): string {
  const badges: string[] = [];
  if (c.id === props.startCellId) badges.push("START");
  if (c.terminal) badges.push("END");
  if (c.status === "draft") badges.push("draft");
  const wrappedName = wrapText(c.name, NODE_TEXT_MAX_WIDTH);
  return badges.length ? `${wrappedName}\n${badges.join(" · ")}` : wrappedName;
}

// cytoscape の height: "label" は要素追加直後のラベル計測タイミングに
// 依存し、fit() が実行される時点でまだ正しい高さが反映されていない
// ことがあった（マスが表示領域外に配置されて見えなくなる不具合の一因）。
// 折り返し行数は buildLabel の時点で確定しているため、高さも自前で
// 計算して `data(height)` として明示的に渡す（cytoscapeの非同期計測に
// 依存しない）。NODE_LINE_HEIGHT はスタイルの line-height（1.6倍）に
// 合わせて計算する（font-size 12px × 1.6 ≒ 19.2px に余裕を足した値）。
const NODE_LINE_HEIGHT = 22;
const NODE_VERTICAL_PADDING = 24;

function buildHeight(label: string): number {
  const lineCount = label.split("\n").length;
  return lineCount * NODE_LINE_HEIGHT + NODE_VERTICAL_PADDING;
}

function buildElements(): cytoscape.ElementDefinition[] {
  const nodes: cytoscape.ElementDefinition[] = props.cells.map((c) => {
    const label = buildLabel(c);
    return {
      data: { id: c.id, label, height: buildHeight(label) },
      classes: [
        c.id === props.startCellId ? "start" : "",
        c.terminal ? "terminal" : "",
        c.status === "draft" ? "draft" : "",
      ]
        .filter(Boolean)
        .join(" "),
    };
  });

  const edges: cytoscape.ElementDefinition[] = props.edges.map((e, i) => ({
    data: {
      id: `e-${i}`,
      source: e.from,
      target: e.to,
      label: e.guard ? `${e.label} [${e.guard}]` : e.label,
      // 削除時に元エッジを特定するための生データ
      origLabel: e.label,
      origGuard: e.guard,
    },
  }));

  return [...nodes, ...edges];
}

// ── レイアウトとスタイル ──────────────────────────────────────────────

const DAGRE_OPTIONS: cytoscape.LayoutOptions = {
  name: "dagre",
  // @ts-expect-error cytoscape-dagre options
  rankDir: "LR",
  nodeSep: 50,
  rankSep: 140,
  padding: 40,
};

const STYLES: cytoscape.CytoscapeOptions["style"] = [
  {
    selector: "node",
    style: {
      "background-color": "#ffffff",
      "border-color": "#d1d5db",
      "border-width": "2px",
      label: "data(label)",
      "text-valign": "center",
      "text-halign": "center",
      width: "150px",
      height: "data(height)",
      shape: "roundrectangle",
      "font-size": "12px",
      color: "#1f2937",
      "text-wrap": "wrap",
      "text-max-width": "140px",
      "line-height": 1.6,
    },
  },
  {
    selector: "node.start",
    style: { "border-color": "#22c55e", "border-width": "3px" },
  },
  {
    selector: "node.terminal",
    style: { "border-color": "#6b7280", "border-width": "3px" },
  },
  {
    selector: "node.draft",
    style: { "background-color": "#fefce8", "border-color": "#facc15" },
  },
  {
    selector: "node:selected",
    style: { "border-color": "#3b82f6", "border-width": "3px" },
  },
  {
    // エッジ選択の視覚強調（Delete 対象を明示）
    selector: "edge:selected",
    style: { "line-color": "#3b82f6", "target-arrow-color": "#3b82f6", width: "3.5px" },
  },
  {
    // エッジ描画中のプレビュー・起点/対象ノードの強調（Excalidraw 風）
    selector: ".eh-source, .eh-target, .eh-hover",
    style: { "border-color": "#f97316", "border-width": "4px", "background-color": "#fff7ed" },
  },
  {
    selector: ".eh-preview, .eh-ghost-edge",
    style: {
      "line-color": "#f97316",
      "target-arrow-color": "#f97316",
      "target-arrow-shape": "triangle",
      "line-style": "dashed",
    },
  },
  {
    selector: "edge",
    style: {
      width: "2px",
      "line-color": "#9ca3af",
      "target-arrow-color": "#9ca3af",
      "target-arrow-shape": "triangle",
      "curve-style": "bezier",
      label: "data(label)",
      "font-size": "10px",
      color: "#6b7280",
      "text-background-color": "#f9fafb",
      "text-background-opacity": 1,
      "text-background-padding": "3px",
      "text-rotation": "autorotate",
    },
  },
];

// ── ピンマーカー（HTML オーバーレイ） ────────────────────────────────

function tabNameFromPath(path: string | null): string {
  if (!path) return "?";
  const parts = path.replace(/\\/g, "/").split("/");
  return parts[parts.length - 1] || "?";
}

function computeMarkerPositions() {
  if (!cy || !props.activeRuns?.length) {
    renderedMarkers.value = [];
    return;
  }
  renderedMarkers.value = props.activeRuns.flatMap((r) => {
    const cell = cy!.getElementById(r.current_cell_id);
    if (!cell.length) return [];
    const pos = cell.renderedPosition();
    const halfH = cell.renderedHeight() / 2;
    return [{
      run_id: r.run_id,
      tabName: tabNameFromPath(r.project_path),
      x: pos.x,
      y: pos.y - halfH - 4,
    }];
  });
}

// ── 配置決定ロジック ─────────────────────────────────────────────────

/// 保存済み配置に無い新規ノードを、既存レイアウトを崩さずに配置する。
///
/// 既存ノード群の外接矩形の「下」に、左詰めで横に並べて置く。これにより
/// マス追加時に dagre 全再計算が走って既存配置が破壊される問題（#28）を防ぐ。
function placeNewNodes(saved: PositionMap, missingIds: string[]) {
  if (!cy) return;
  const positioned = cy.nodes().filter((n) => saved[n.id()] != null);

  let baseX = 0;
  let baseY = 0;
  if (positioned.length > 0) {
    const bb = positioned.boundingBox();
    baseX = bb.x1;
    baseY = bb.y2 + 90; // 既存群の下に余白を空けて配置
  }

  const GAP_X = 200;
  missingIds.forEach((id, i) => {
    cy!.getElementById(id).position({ x: baseX + i * GAP_X, y: baseY });
  });
}

/// 全体が収まるようフィットしつつ、拡大しすぎないようズーム率を上限で抑える。
///
/// ノードが1〜数個だけのとき cy.fit() は画面いっぱいに拡大して不恰好になるため、
/// 等倍（1.0）を上限にクランプし、はみ出さない場合は中央寄せする。
///
/// cy.resize() を先に呼び、cytoscape内部に現在のコンテナの実サイズを
/// 再認識させてから fit する。マウント直後などflexboxレイアウトが
/// まだ確定していないタイミングでfitが走ると、古い（小さい）サイズを
/// 基準に計算してしまい、一部のノードが実際の表示領域外に配置されて
/// 見えなくなることがあったため。
const MAX_FIT_ZOOM = 1.0;
function fitView() {
  if (!cy) return;
  cy.resize();
  cy.fit(undefined, 40);
  if (cy.zoom() > MAX_FIT_ZOOM) {
    cy.zoom(MAX_FIT_ZOOM);
    cy.center();
  }
}

function placeNodes() {
  if (!cy) return;
  const saved = loadPositions();
  const missingIds = props.cells
    .filter((c) => !saved?.[c.id])
    .map((c) => c.id);

  if (saved && missingIds.length < props.cells.length) {
    // 既存の手動配置がある: それを保持し、未配置ノードのみ追加配置する
    applyPositions(saved);
    if (missingIds.length > 0) {
      placeNewNodes(saved, missingIds);
      savePositions();
    }
    fitView();
    computeMarkerPositions();
  } else {
    // 保存が全く無い（初回）→ dagre で自動レイアウト
    const layout = cy.layout(DAGRE_OPTIONS);
    layout.one("layoutstop", () => {
      savePositions();
      fitView();
      computeMarkerPositions();
    });
    layout.run();
  }
}

// ── 初期化・更新 ──────────────────────────────────────────────────────

function initCy() {
  if (!container.value) return;
  cy = cytoscape({
    container: container.value,
    elements: buildElements(),
    style: STYLES,
    layout: { name: "null" },
    wheelSensitivity: 0.3,
  });
  // 単一クリック: ノード=選択+詳細表示 / エッジ=選択
  cy.on("tap", "node", (evt) => {
    selected.value = { type: "node", id: evt.target.id() as string };
    emit("select", evt.target.id() as string);
  });
  cy.on("tap", "edge", (evt) => {
    selected.value = { type: "edge", id: evt.target.id() as string };
  });
  // 背景クリックで選択解除
  cy.on("tap", (evt) => {
    if (evt.target === cy) selected.value = null;
  });

  // ダブルクリック: ノード=その場改名 / エッジ=ラベル+ガード編集
  cy.on("dbltap", "node", (evt) => showNodeEditor(evt.target as cytoscape.NodeSingular));
  cy.on("dbltap", "edge", (evt) => {
    const d = evt.target.data();
    const screen = toScreen(evt.renderedPosition ?? { x: 0, y: 0 });
    emit("edgeEdit", {
      from: d.source as string,
      to: d.target as string,
      label: d.origLabel as string,
      guard: (d.origGuard as string | null) ?? null,
      x: screen.x,
      y: screen.y,
    });
  });

  cy.on("dragfree", "node", () => {
    savePositions();
    computeMarkerPositions();
  });
  // パン・ズーム・ドラッグ中は接続ハンドル・改名入力を退避（古い座標に残さない）
  cy.on("viewport", () => { handle.value = null; nodeEditor.value = null; computeMarkerPositions(); });
  cy.on("drag", "node", () => { handle.value = null; nodeEditor.value = null; });

  // 接続ハンドル: ノードにホバーで右縁に表示
  cy.on("mouseover", "node", (evt) => updateHandle(evt.target as cytoscape.NodeSingular));
  cy.on("mouseout", "node", () => { handle.value = null; });

  // ── エッジハンドル（ハンドルからドラッグして接続）──────────────────
  // ドローモードは使わず、接続ハンドルの mousedown で eh.start() を呼ぶ。
  eh = cy.edgehandles({
    snap: true,
    canConnect: (source, target) => !source.same(target),
    edgeParams: () => ({}),
  });
  // ドラッグ完了で仮エッジが追加される。永続化はバックエンド経由で行うため
  // 仮エッジは即削除し、ラベル入力のため connect を通知する。
  cy.on("ehcomplete", (evt, source, target, addedEdge) => {
    (addedEdge as cytoscape.EdgeSingular).remove();
    const oe = (evt as unknown as { originalEvent?: MouseEvent }).originalEvent;
    emit("connect", {
      from: source.id() as string,
      to: target.id() as string,
      x: oe?.clientX ?? window.innerWidth / 2,
      y: oe?.clientY ?? window.innerHeight / 2,
    });
  });

  // マウント直後は親のflexboxレイアウトがまだ確定していないことがあり、
  // その状態で fit すると古い（小さい）コンテナサイズを基準に計算されて
  // 一部のノードが実際の表示領域外に配置されることがあった。
  // requestAnimationFrame で最低1回描画を経てからレイアウトを確定させる。
  requestAnimationFrame(() => placeNodes());
}

/// cytoscape のレンダリング座標（コンテナ相対）を画面座標へ変換する。
function toScreen(rp: { x: number; y: number }): { x: number; y: number } {
  const rect = container.value?.getBoundingClientRect();
  return { x: (rect?.left ?? 0) + rp.x, y: (rect?.top ?? 0) + rp.y };
}

/// ホバー中ノードの右縁に接続ハンドルを表示する。
function updateHandle(node: cytoscape.NodeSingular) {
  const pos = node.renderedPosition();
  const halfW = node.renderedWidth() / 2;
  handle.value = { x: pos.x + halfW, y: pos.y, nodeId: node.id() as string };
}

/// 接続ハンドルの mousedown からエッジ描画ジェスチャを開始する。
function startConnect() {
  if (!handle.value || !eh || !cy) return;
  const node = cy.getElementById(handle.value.nodeId);
  eh.start(node as cytoscape.NodeSingular);
  handle.value = null;
}

/// ノード名インライン編集を開く。
function showNodeEditor(node: cytoscape.NodeSingular) {
  const pos = node.renderedPosition();
  nodeEditor.value = {
    cellId: node.id() as string,
    name: props.cells.find((c) => c.id === node.id())?.name ?? "",
    x: pos.x,
    y: pos.y,
    width: node.renderedWidth(),
  };
}

function onNodeNameCommit(name: string) {
  if (nodeEditor.value) emit("nodeRename", { cellId: nodeEditor.value.cellId, name });
  nodeEditor.value = null;
}

/// Delete / Backspace で選択要素を削除する。
function onKeydown(e: KeyboardEvent) {
  if (e.key !== "Delete" && e.key !== "Backspace") return;
  if ((e.target as HTMLElement)?.tagName === "INPUT") return; // 入力中は無視
  if (!selected.value || !cy) return;
  const el = cy.getElementById(selected.value.id);
  if (!el || el.length === 0) return;
  if (selected.value.type === "edge") {
    const d = el.data();
    emit("edgeDelete", {
      from: d.source as string,
      to: d.target as string,
      label: d.origLabel as string,
      guard: (d.origGuard as string | null) ?? null,
    });
  } else {
    emit("nodeDelete", selected.value.id);
  }
  selected.value = null;
}

function refresh() {
  if (!cy) return;
  handle.value = null;
  nodeEditor.value = null;
  selected.value = null;
  cy.elements().remove();
  cy.add(buildElements());
  placeNodes();
}

function onResize() {
  if (!cy || !container.value) return;
  // fitView() 内で cy.resize() を呼ぶため、ここでは呼ばない。
  fitView();
  computeMarkerPositions();
}

onMounted(() => {
  initCy();
  window.addEventListener("resize", onResize);
  container.value?.addEventListener("keydown", onKeydown);
});
onUnmounted(() => {
  window.removeEventListener("resize", onResize);
  container.value?.removeEventListener("keydown", onKeydown);
  eh?.destroy();
  eh = null;
  cy?.destroy();
  cy = null;
});

watch(() => [props.cells, props.edges, props.startCellId], refresh, { deep: true });
watch(() => props.activeRuns, computeMarkerPositions, { deep: true });

defineExpose({ buildLabel, buildElements, fitView });
</script>
