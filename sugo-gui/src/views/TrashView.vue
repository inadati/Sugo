<template>
  <div>
    <h2 class="text-lg font-semibold mb-4">ゴミ箱</h2>

    <ul v-if="trashItems.length > 0" class="space-y-2">
      <li
        v-for="item in trashItems"
        :key="item.harness_id"
        class="bg-white rounded border border-gray-200 px-4 py-3 flex items-center justify-between"
      >
        <div>
          <p class="font-medium">{{ item.name }}</p>
          <p class="text-xs text-gray-400 mt-0.5">
            削除日: {{ formatDate(item.deleted_at) }}
            <span :class="item.remaining_days <= 30 ? 'text-red-500 font-medium' : ''">
              あと{{ item.remaining_days }}日
            </span>
          </p>
        </div>
        <div class="flex items-center gap-2">
          <button
            data-testid="restore-btn"
            class="px-3 py-1 text-sm border border-gray-300 rounded hover:bg-gray-50 text-gray-700"
            @click="restore(item)"
          >
            復活
          </button>
          <button
            data-testid="purge-btn"
            class="px-3 py-1 text-sm text-red-500 border border-red-200 rounded hover:bg-red-50"
            @click="confirmPurge(item)"
          >
            完全削除
          </button>
        </div>
      </li>
    </ul>

    <p v-else class="text-gray-400">ゴミ箱は空です</p>

    <!-- 完全削除確認ダイアログ -->
    <div
      v-if="purgeTarget"
      data-testid="purge-dialog"
      class="fixed inset-0 bg-black/30 z-50 flex items-center justify-center"
    >
      <div class="bg-white rounded-lg shadow-lg p-6 w-80">
        <p class="text-sm font-medium mb-2">
          "{{ purgeTarget.name }}" は完全に削除されます。元に戻せません。
        </p>
        <div class="flex gap-2 justify-end">
          <button
            data-testid="purge-cancel-btn"
            class="px-3 py-1.5 text-sm text-gray-600 hover:text-gray-900"
            @click="purgeTarget = null"
          >
            キャンセル
          </button>
          <button
            data-testid="purge-confirm-btn"
            class="px-3 py-1.5 text-sm bg-red-500 text-white rounded hover:bg-red-600"
            @click="doPurge"
          >
            完全削除
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";

interface TrashItem {
  harness_id: string;
  name: string;
  deleted_at: string;
  remaining_days: number;
}

const POLL_INTERVAL_MS = 2000;
const trashItems = ref<TrashItem[]>([]);
const purgeTarget = ref<TrashItem | null>(null);
let pollTimer: ReturnType<typeof setInterval> | null = null;

async function fetchTrash() {
  trashItems.value = await invoke<TrashItem[]>("list_trash");
}

async function restore(item: TrashItem) {
  await invoke("restore_harness", { harnessId: item.harness_id });
  await fetchTrash();
}

function confirmPurge(item: TrashItem) {
  purgeTarget.value = item;
}

async function doPurge() {
  if (!purgeTarget.value) return;
  await invoke("purge_harness", { harnessId: purgeTarget.value.harness_id });
  purgeTarget.value = null;
  await fetchTrash();
}

function formatDate(iso: string): string {
  return iso.slice(0, 10);
}

onMounted(() => {
  void fetchTrash();
  pollTimer = setInterval(fetchTrash, POLL_INTERVAL_MS);
});

onUnmounted(() => {
  if (pollTimer) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
});
</script>
