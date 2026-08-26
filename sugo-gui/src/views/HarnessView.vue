<template>
  <div v-if="detail" class="flex flex-col h-full">
    <div class="shrink-0 flex items-center justify-between px-4 pt-3 pb-3">
      <div>
        <button class="text-gray-400 text-sm hover:text-gray-600 mb-1" @click="router.push('/')">← 一覧</button>
        <h2 class="text-xl font-semibold">{{ detail.name }}</h2>
        <p class="text-sm text-gray-400">v{{ detail.current_version }}</p>
      </div>
      <button
        class="bg-blue-500 text-white px-4 py-2 rounded hover:bg-blue-600"
        @click="showAddCell = true"
      >+ マスを追加</button>
    </div>

    <!-- 操作ヒント（モードレス編集） -->
    <div class="shrink-0 mb-3 bg-gray-50 border border-gray-200 rounded px-3 py-2 text-xs text-gray-500">
      ノードの縁の●からドラッグして接続 ／ ダブルクリックで名前・エッジを編集 ／ 選択して Delete で削除
    </div>

    <!-- トースト -->
    <div
      v-if="toast"
      data-testid="toast"
      class="fixed top-4 left-1/2 -translate-x-1/2 z-50 bg-gray-800 text-white text-sm px-4 py-2 rounded shadow"
    >{{ toast }}</div>

    <!-- 盤面グラフ -->
    <BoardGraph
      class="flex-1 min-h-0"
      :harness-id="detail.harness_id"
      :cells="detail.cells"
      :edges="detail.edges"
      :start-cell-id="detail.start_cell_id"
      :active-runs="activeRuns"
      @select="onSelectCell"
      @connect="onConnect"
      @edge-edit="onEdgeEdit"
      @edge-delete="onEdgeDelete"
      @node-delete="onNodeDelete"
      @node-rename="onNodeRename"
    />

    <!-- マス詳細パネル: パネル外クリックで閉じるオーバーレイ -->
    <template v-if="selectedCell">
      <div class="fixed inset-0 z-30" @click="selectedCellId = null" />
      <CellDetailPanel
        :harness-id="detail.harness_id"
        :cell="selectedCell"
        :is-start="selectedCell.id === detail.start_cell_id"
        :lock-version="lockVersion"
        @close="selectedCellId = null"
        @renamed="onCellRenamed"
        @deleted="onCellDeleted"
      />
    </template>

    <!-- マス追加ダイアログ -->
    <AddCellDialog
      v-if="showAddCell"
      :harness-id="detail.harness_id"
      :lock-version="lockVersion"
      @close="showAddCell = false"
      @added="onCellAdded"
    />

    <!-- エッジ追加・編集ポップオーバー -->
    <EdgeEditor
      v-if="edgeEditor"
      :harness-id="detail.harness_id"
      :mode="edgeEditor.mode"
      :from="edgeEditor.from"
      :to="edgeEditor.to"
      :from-name="cellName(edgeEditor.from)"
      :to-name="cellName(edgeEditor.to)"
      :old-label="edgeEditor.oldLabel"
      :initial-label="edgeEditor.initialLabel"
      :initial-guard="edgeEditor.initialGuard"
      :anchor-x="edgeEditor.x"
      :anchor-y="edgeEditor.y"
      :lock-version="lockVersion"
      @close="edgeEditor = null"
      @saved="onEdgeSaved"
      @reload="onEditorReload"
      @delete="onEdgeEditorDelete"
    />
  </div>
  <div v-else class="text-gray-400">読み込み中...</div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useRouter } from "vue-router";
import BoardGraph from "../components/BoardGraph.vue";
import AddCellDialog from "../components/AddCellDialog.vue";
import CellDetailPanel from "../components/CellDetailPanel.vue";
import EdgeEditor from "../components/EdgeEditor.vue";
import { useToast } from "../composables/useToast";

const props = defineProps<{ id: string }>();
const router = useRouter();

interface Cell { id: string; name: string; prompt: string; status: string; terminal: boolean; memo: string }

interface HarnessDetail {
  harness_id: string;
  name: string;
  current_version: number;
  lock_version: number;
  has_draft: boolean;
  start_cell_id: string;
  cells: Cell[];
  edges: { from: string; to: string; label: string; guard: string | null }[];
  draft_diff: { cell_id: string; name: string; memo: string }[];
}

interface ActiveRun {
  run_id: string;
  current_cell_id: string;
  project_path: string | null;
}

type EdgeEditorState = {
  mode: "add" | "edit";
  from: string;
  to: string;
  oldLabel?: string;
  initialLabel?: string;
  initialGuard?: string | null;
  x: number;
  y: number;
};

const detail = ref<HarnessDetail | null>(null);
const lockVersion = ref(0);
const showAddCell = ref(false);
const selectedCellId = ref<string | null>(null);
const activeRuns = ref<ActiveRun[]>([]);
const edgeEditor = ref<EdgeEditorState | null>(null);
const { toast, showToast } = useToast();

const selectedCell = computed<Cell | null>(
  () => detail.value?.cells.find((c) => c.id === selectedCellId.value) ?? null
);

function handleMutationError(e: unknown) {
  const msg = String(e);
  if (msg.includes("cannot_delete_start")) {
    showToast("START マスは削除できません。");
  } else if (msg.includes("lock_conflict")) {
    showToast("他で編集が入りました。再読み込みします。");
    void load();
  } else if (msg.includes("edge_not_found") || msg.includes("not found")) {
    // 並行編集で盤面がずれた系は再読込で自己修復する（設計 line 79）
    showToast("盤面が更新されていました。再読み込みします。");
    void load();
  } else {
    showToast("操作に失敗しました。");
  }
}

async function load() {
  const [d, runs] = await Promise.all([
    invoke<HarnessDetail>("get_harness", { harnessId: props.id }),
    invoke<ActiveRun[]>("get_active_runs", { harnessId: props.id }),
  ]);
  detail.value = d;
  lockVersion.value = d.lock_version;
  activeRuns.value = runs;
}

function onSelectCell(cellId: string) {
  selectedCellId.value = cellId;
}

function cellName(cellId: string): string {
  return detail.value?.cells.find((c) => c.id === cellId)?.name ?? cellId;
}

function onConnect(p: { from: string; to: string; x: number; y: number }) {
  edgeEditor.value = { mode: "add", from: p.from, to: p.to, x: p.x, y: p.y };
}

function onEdgeEdit(p: { from: string; to: string; label: string; guard: string | null; x: number; y: number }) {
  edgeEditor.value = {
    mode: "edit",
    from: p.from,
    to: p.to,
    oldLabel: p.label,
    initialLabel: p.label,
    initialGuard: p.guard,
    x: p.x,
    y: p.y,
  };
}

async function onEdgeSaved(_newVersion: number, newLockVersion: number) {
  edgeEditor.value = null;
  lockVersion.value = newLockVersion;
  await load();
}

async function onEditorReload(message: string) {
  edgeEditor.value = null;
  showToast(message);
  await load();
}

async function onEdgeEditorDelete(payload: { from: string; to: string; label: string }) {
  edgeEditor.value = null;
  await onEdgeDelete(payload);
}

async function onEdgeDelete(payload: { from: string; to: string; label: string }) {
  try {
    const result = await invoke<{ new_version: number; lock_version: number }>("delete_edge", {
      harnessId: props.id,
      from: payload.from,
      to: payload.to,
      label: payload.label,
      lockVersion: lockVersion.value,
    });
    lockVersion.value = result.lock_version;
    await load();
  } catch (e) {
    handleMutationError(e);
  }
}

async function onNodeDelete(cellId: string) {
  try {
    const result = await invoke<{ new_version: number; lock_version: number }>("delete_cell", {
      harnessId: props.id,
      cellId,
      lockVersion: lockVersion.value,
    });
    if (selectedCellId.value === cellId) selectedCellId.value = null;
    lockVersion.value = result.lock_version;
    await load();
  } catch (e) {
    handleMutationError(e);
  }
}

async function onNodeRename(p: { cellId: string; name: string }) {
  try {
    const result = await invoke<{ new_version: number; lock_version: number }>("rename_cell", {
      harnessId: props.id,
      cellId: p.cellId,
      newName: p.name,
      lockVersion: lockVersion.value,
    });
    lockVersion.value = result.lock_version;
    await load();
  } catch (e) {
    handleMutationError(e);
  }
}

async function onCellAdded(_newVersion: number, newLockVersion: number) {
  showAddCell.value = false;
  lockVersion.value = newLockVersion;
  await load();
}

async function onCellRenamed(_newVersion: number, newLockVersion: number) {
  lockVersion.value = newLockVersion;
  await load();
}

async function onCellDeleted(_newVersion: number, newLockVersion: number) {
  selectedCellId.value = null;
  lockVersion.value = newLockVersion;
  await load();
}

// ── ポーリング（#15）: current_version 変化時のみ再描画。非アクティブ時は停止 ──
const POLL_INTERVAL_MS = 2000;
let pollTimer: ReturnType<typeof setInterval> | null = null;

async function poll() {
  if (document.hidden) return;
  try {
    const [latest, runs] = await Promise.all([
      invoke<HarnessDetail>("get_harness", { harnessId: props.id }),
      invoke<ActiveRun[]>("get_active_runs", { harnessId: props.id }),
    ]);
    if (!detail.value || latest.current_version !== detail.value.current_version) {
      detail.value = latest;
      lockVersion.value = latest.lock_version;
    }
    activeRuns.value = runs;
  } catch {
    // 1 回の失敗は無視（次周期で自己修復）
  }
}

function startPolling() {
  if (pollTimer) return;
  pollTimer = setInterval(poll, POLL_INTERVAL_MS);
}
function stopPolling() {
  if (pollTimer) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
}
function onVisibilityChange() {
  if (document.hidden) {
    stopPolling();
  } else {
    startPolling();
    void poll();
  }
}

onMounted(() => {
  void load();
  startPolling();
  document.addEventListener("visibilitychange", onVisibilityChange);
});
onUnmounted(() => {
  stopPolling();
  document.removeEventListener("visibilitychange", onVisibilityChange);
});
</script>
