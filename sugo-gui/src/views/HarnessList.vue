<template>
  <div>
    <h2 class="text-lg font-semibold mb-4">ハーネス一覧</h2>
    <ul class="space-y-2">
      <li
        v-for="h in harnesses"
        :key="h.harness_id"
        class="group bg-white rounded border border-gray-200 px-4 py-3 cursor-pointer hover:bg-gray-50 flex items-center justify-between"
        @click="router.push(`/harness/${h.harness_id}`)"
      >
        <span class="font-medium">{{ h.name }}</span>
        <div class="flex items-center gap-2">
          <span class="text-sm text-gray-400">v{{ h.current_version }}</span>
          <span
            v-if="h.has_draft"
            class="text-xs bg-yellow-100 text-yellow-800 px-2 py-0.5 rounded font-bold"
          >
            DRAFT
          </span>
          <button
            data-testid="trash-btn"
            class="opacity-0 group-hover:opacity-100 p-1 text-gray-400 hover:text-red-500 transition-opacity"
            @click.stop="confirmTrash(h)"
          >
            <TrashIcon class="w-4 h-4" />
          </button>
        </div>
      </li>
    </ul>
    <p v-if="harnesses.length === 0" class="text-gray-400">
      ハーネスがありません。MCP の sugo_create_harness で作成してください。
    </p>

    <!-- 確認ダイアログ -->
    <div
      v-if="trashTarget"
      data-testid="trash-dialog"
      class="fixed inset-0 bg-black/30 z-50 flex items-center justify-center"
    >
      <div class="bg-white rounded-lg shadow-lg p-6 w-80">
        <p class="text-sm font-medium mb-4">
          "{{ trashTarget.name }}" をゴミ箱に移動しますか？
        </p>
        <p v-if="trashError" class="text-xs text-red-500 mb-3">{{ trashError }}</p>
        <div class="flex gap-2 justify-end">
          <button
            data-testid="trash-cancel-btn"
            class="px-3 py-1.5 text-sm text-gray-600 hover:text-gray-900"
            @click="trashTarget = null; trashError = null"
          >
            キャンセル
          </button>
          <button
            class="px-3 py-1.5 text-sm bg-red-500 text-white rounded hover:bg-red-600"
            @click="doTrash"
          >
            移動する
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useRouter } from "vue-router";
import { TrashIcon } from "@heroicons/vue/24/outline";

interface HarnessSummary {
  harness_id: string;
  name: string;
  current_version: number;
  has_draft: boolean;
}

const POLL_INTERVAL_MS = 2000;
const router = useRouter();
const harnesses = ref<HarnessSummary[]>([]);
const trashTarget = ref<HarnessSummary | null>(null);
const trashError = ref<string | null>(null);
let pollTimer: ReturnType<typeof setInterval> | null = null;

async function fetchHarnesses() {
  harnesses.value = await invoke<HarnessSummary[]>("list_harnesses");
}

function confirmTrash(h: HarnessSummary) {
  trashTarget.value = h;
  trashError.value = null;
}

async function doTrash() {
  if (!trashTarget.value) return;
  try {
    await invoke("trash_harness", { harnessId: trashTarget.value.harness_id });
    trashTarget.value = null;
    await fetchHarnesses();
  } catch (e: unknown) {
    trashError.value =
      e === "active_run"
        ? "実行中のRunがあるため移動できません"
        : String(e);
  }
}

onMounted(() => {
  void fetchHarnesses();
  pollTimer = setInterval(fetchHarnesses, POLL_INTERVAL_MS);
});
onUnmounted(() => {
  if (pollTimer) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
});
</script>
