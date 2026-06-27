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
import { ref, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useRouter } from "vue-router";
import BoardGraph from "../components/BoardGraph.vue";
import AddCellDialog from "../components/AddCellDialog.vue";

const props = defineProps<{ id: string }>();
const router = useRouter();

interface HarnessDetail {
  harness_id: string;
  name: string;
  current_version: number;
  lock_version: number;
  has_draft: boolean;
  cells: { id: string; name: string; status: string; terminal: boolean }[];
  edges: { from: string; to: string; label: string; guard: string | null }[];
  draft_diff: { cell_id: string; name: string }[];
}

const detail = ref<HarnessDetail | null>(null);
const lockVersion = ref(0);
const showAddCell = ref(false);

async function load() {
  detail.value = await invoke<HarnessDetail>("get_harness", { harnessId: props.id });
  lockVersion.value = detail.value.lock_version;
}

async function onCellAdded(newVersion: number, newLockVersion: number) {
  showAddCell.value = false;
  lockVersion.value = newLockVersion;
  await load();
}

onMounted(load);
</script>
