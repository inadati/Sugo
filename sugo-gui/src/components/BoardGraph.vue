<template>
  <div ref="container" class="relative w-full h-full bg-gray-50 rounded border">
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
  </div>
</template>

<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted } from "vue";
import cytoscape from "cytoscape";
import cytoscapeDagre from "cytoscape-dagre";

cytoscape.use(cytoscapeDagre as cytoscape.Ext);

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
  editMode?: boolean;
}>();

const emit = defineEmits<{
  select: [cellId: string];
  connect: [payload: { from: string; to: string }];
  edgeDelete: [payload: { from: string; to: string; label: string; guard: string | null }];
}>();

// 編集モードでエッジ接続の起点に選んだノード
const pendingSourceId = ref<string | null>(null);

const container = ref<HTMLElement | null>(null);
let cy: cytoscape.Core | null = null;
const renderedMarkers = ref<RenderedMarker[]>([]);

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

function loadPositions(): PositionMap | null {
  try {
    const raw = localStorage.getItem(lsKey());
    return raw ? (JSON.parse(raw) as PositionMap) : null;
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

function buildLabel(c: CellData): string {
  const badges: string[] = [];
  if (c.id === props.startCellId) badges.push("START");
  if (c.terminal) badges.push("END");
  if (c.status === "draft") badges.push("draft");
  return badges.length ? `${c.name}\n${badges.join(" · ")}` : c.name;
}

function buildElements(): cytoscape.ElementDefinition[] {
  const nodes: cytoscape.ElementDefinition[] = props.cells.map((c) => ({
    data: { id: c.id, label: buildLabel(c) },
    classes: [
      c.id === props.startCellId ? "start" : "",
      c.terminal ? "terminal" : "",
      c.status === "draft" ? "draft" : "",
    ]
      .filter(Boolean)
      .join(" "),
  }));

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
      height: "52px",
      shape: "roundrectangle",
      "font-size": "12px",
      color: "#1f2937",
      "text-wrap": "wrap",
      "text-max-width": "140px",
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
    selector: "node.connect-source",
    style: { "border-color": "#f97316", "border-width": "4px", "background-color": "#fff7ed" },
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
    cy.fit(undefined, 40);
    computeMarkerPositions();
  } else {
    // 保存が全く無い（初回）→ dagre で自動レイアウト
    const layout = cy.layout(DAGRE_OPTIONS);
    layout.one("layoutstop", () => {
      savePositions();
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
  cy.on("tap", "node", (evt) => {
    const id = evt.target.id() as string;
    if (!props.editMode) {
      emit("select", id);
      return;
    }
    // 編集モード: 1つ目のノードで起点、2つ目のノードで接続
    if (!pendingSourceId.value) {
      setPendingSource(id);
    } else if (pendingSourceId.value === id) {
      clearPendingSource(); // 同じノード再タップでキャンセル
    } else {
      const from = pendingSourceId.value;
      clearPendingSource();
      emit("connect", { from, to: id });
    }
  });
  cy.on("tap", "edge", (evt) => {
    if (!props.editMode) return;
    const d = evt.target.data();
    emit("edgeDelete", {
      from: d.source as string,
      to: d.target as string,
      label: d.origLabel as string,
      guard: (d.origGuard as string | null) ?? null,
    });
  });
  // 背景タップで起点選択を解除
  cy.on("tap", (evt) => {
    if (evt.target === cy && props.editMode) clearPendingSource();
  });
  cy.on("dragfree", "node", () => {
    savePositions();
    computeMarkerPositions();
  });
  // パン・ズーム時もピン位置を追従
  cy.on("viewport", computeMarkerPositions);
  placeNodes();
}

function setPendingSource(id: string) {
  if (!cy) return;
  clearPendingSource();
  pendingSourceId.value = id;
  cy.getElementById(id).addClass("connect-source");
}

function clearPendingSource() {
  if (cy && pendingSourceId.value) {
    cy.getElementById(pendingSourceId.value).removeClass("connect-source");
  }
  pendingSourceId.value = null;
}

function refresh() {
  if (!cy) return;
  clearPendingSource();
  cy.elements().remove();
  cy.add(buildElements());
  placeNodes();
}

function onResize() {
  if (!cy || !container.value) return;
  cy.resize();
  cy.fit(undefined, 40);
  computeMarkerPositions();
}

onMounted(() => {
  initCy();
  window.addEventListener("resize", onResize);
});
onUnmounted(() => {
  window.removeEventListener("resize", onResize);
  cy?.destroy();
  cy = null;
});

watch(() => [props.cells, props.edges, props.startCellId], refresh, { deep: true });
watch(() => props.activeRuns, computeMarkerPositions, { deep: true });
// 編集モードを抜けたら起点選択を解除
watch(() => props.editMode, (on) => { if (!on) clearPendingSource(); });

defineExpose({ buildLabel, buildElements });
</script>
