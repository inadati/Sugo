<template>
  <div v-if="detail" class="flex flex-col h-full">
    <div class="shrink-0 flex items-center justify-between px-4 pt-3 pb-3">
      <div>
        <button class="text-gray-400 text-sm hover:text-gray-600 mb-1" @click="router.push('/')">← 一覧</button>
        <h2 class="text-xl font-semibold">{{ detail.name }}</h2>
        <p class="text-sm text-gray-400">v{{ detail.current_version }}</p>
      </div>
      <div class="flex items-center gap-2">
        <button
          data-testid="toggle-edit"
          class="px-4 py-2 rounded border"
          :class="editMode
            ? 'bg-orange-500 text-white border-orange-500 hover:bg-orange-600'
            : 'bg-white text-gray-700 border-gray-300 hover:bg-gray-50'"
          @click="editMode = !editMode"
        >{{ editMode ? '編集モード: ON' : '編集モード' }}</button>
        <button
          class="bg-blue-500 text-white px-4 py-2 rounded hover:bg-blue-600"
          @click="showAddCell = true"
        >+ マスを追加</button>
      </div>
    </div>

    <!-- 編集モードの操作ヒント -->
    <div v-if="editMode" class="shrink-0 mb-3 bg-orange-50 border border-orange-200 rounded px-3 py-2 text-sm text-orange-800">
      編集モード: ノードを2つ順にクリックするとエッジを追加できます（起点→終点）。エッジをクリックすると削除できます。
    </div>

    <!-- ドラフト差分 -->
    <div v-if="detail.draft_diff.length > 0" class="shrink-0 mb-3 bg-yellow-50 border border-yellow-200 rounded p-3">
      <p class="text-sm font-medium text-yellow-800 mb-1">ドラフトセル（エージェントへ共有）</p>
      <ul class="text-sm text-yellow-700 space-y-0.5">
        <li v-for="d in detail.draft_diff" :key="d.cell_id">・{{ d.name }} ({{ d.cell_id }})</li>
      </ul>
    </div>

    <!-- 盤面グラフ -->
    <BoardGraph
      class="flex-1 min-h-0"
      :harness-id="detail.harness_id"
      :cells="detail.cells"
      :edges="detail.edges"
      :start-cell-id="detail.cells[0]?.id ?? ''"
      :active-runs="activeRuns"
      :edit-mode="editMode"
      @select="onSelectCell"
      @connect="onConnect"
      @edge-delete="onEdgeDelete"
    />

    <!-- マス詳細パネル: パネル外クリックで閉じるオーバーレイ -->
    <template v-if="selectedCell">
      <div class="fixed inset-0 z-30" @click="selectedCellId = null" />
      <CellDetailPanel
        :harness-id="detail.harness_id"
        :cell="selectedCell"
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

    <!-- エッジ追加ダイアログ -->
    <EdgeDialog
      v-if="pendingEdge"
      :harness-id="detail.harness_id"
      :from="pendingEdge.from"
      :to="pendingEdge.to"
      :from-name="cellName(pendingEdge.from)"
      :to-name="cellName(pendingEdge.to)"
      :lock-version="lockVersion"
      @close="pendingEdge = null"
      @added="onEdgeAdded"
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
import EdgeDialog from "../components/EdgeDialog.vue";

const props = defineProps<{ id: string }>();
const router = useRouter();

interface Cell { id: string; name: string; prompt: string; status: string; terminal: boolean }

interface HarnessDetail {
  harness_id: string;
  name: string;
  current_version: number;
  lock_version: number;
  has_draft: boolean;
  cells: Cell[];
  edges: { from: string; to: string; label: string; guard: string | null }[];
  draft_diff: { cell_id: string; name: string }[];
}

interface ActiveRun {
  run_id: string;
  current_cell_id: string;
  project_path: string | null;
}

const detail = ref<HarnessDetail | null>(null);
const lockVersion = ref(0);
const showAddCell = ref(false);
const selectedCellId = ref<string | null>(null);
const activeRuns = ref<ActiveRun[]>([]);
const editMode = ref(false);
const pendingEdge = ref<{ from: string; to: string } | null>(null);

const selectedCell = computed<Cell | null>(
  () => detail.value?.cells.find((c) => c.id === selectedCellId.value) ?? null
);

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

function onConnect(payload: { from: string; to: string }) {
  pendingEdge.value = payload;
}

async function onEdgeAdded(_newVersion: number, newLockVersion: number) {
  pendingEdge.value = null;
  lockVersion.value = newLockVersion;
  await load();
}

async function onEdgeDelete(payload: { from: string; to: string; label: string; guard: string | null }) {
  const from = cellName(payload.from);
  const to = cellName(payload.to);
  if (!confirm(`エッジ「${payload.label}」（${from} → ${to}）を削除しますか？`)) return;
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
    const msg = String(e);
    if (msg.includes("lock_conflict")) {
      alert("他で編集が入りました。再読み込みしてください。");
    } else {
      alert("エッジの削除に失敗しました。");
    }
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
