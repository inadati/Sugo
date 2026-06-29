<template>
  <div v-if="detail">
    <div class="flex items-center justify-between mb-4">
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

    <!-- ドラフト差分 -->
    <div v-if="detail.draft_diff.length > 0" class="mb-4 bg-yellow-50 border border-yellow-200 rounded p-3">
      <p class="text-sm font-medium text-yellow-800 mb-1">ドラフトセル（エージェントへ共有）</p>
      <ul class="text-sm text-yellow-700 space-y-0.5">
        <li v-for="d in detail.draft_diff" :key="d.cell_id">・{{ d.name }} ({{ d.cell_id }})</li>
      </ul>
    </div>

    <!-- 盤面グラフ -->
    <BoardGraph
      :cells="detail.cells"
      :edges="detail.edges"
      :start-cell-id="detail.cells[0]?.id ?? ''"
      @select="onSelectCell"
    />

    <!-- マス詳細パネル -->
    <CellDetailPanel
      v-if="selectedCell"
      :harness-id="detail.harness_id"
      :cell="selectedCell"
      :lock-version="lockVersion"
      @close="selectedCellId = null"
      @renamed="onCellRenamed"
    />

    <!-- マス追加ダイアログ -->
    <AddCellDialog
      v-if="showAddCell"
      :harness-id="detail.harness_id"
      :lock-version="lockVersion"
      @close="showAddCell = false"
      @added="onCellAdded"
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

const detail = ref<HarnessDetail | null>(null);
const lockVersion = ref(0);
const showAddCell = ref(false);
const selectedCellId = ref<string | null>(null);

const selectedCell = computed<Cell | null>(
  () => detail.value?.cells.find((c) => c.id === selectedCellId.value) ?? null
);

async function load() {
  detail.value = await invoke<HarnessDetail>("get_harness", { harnessId: props.id });
  lockVersion.value = detail.value.lock_version;
}

function onSelectCell(cellId: string) {
  selectedCellId.value = cellId;
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

// ── ポーリング（#15）: current_version 変化時のみ再描画。非アクティブ時は停止 ──
const POLL_INTERVAL_MS = 2000;
let pollTimer: ReturnType<typeof setInterval> | null = null;

async function poll() {
  if (document.hidden) return;
  try {
    const latest = await invoke<HarnessDetail>("get_harness", { harnessId: props.id });
    if (!detail.value || latest.current_version !== detail.value.current_version) {
      detail.value = latest;
      lockVersion.value = latest.lock_version;
    }
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
