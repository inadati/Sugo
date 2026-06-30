<template>
  <nav class="w-[140px] shrink-0 bg-white border-r border-gray-200 flex flex-col py-2 gap-0.5">
    <RouterLink
      to="/"
      exact-active-class="bg-gray-100 font-medium"
      class="flex items-center gap-2 px-3 py-2 text-sm rounded mx-1 hover:bg-gray-100 text-gray-700"
    >
      <ListBulletIcon class="w-4 h-4 shrink-0" />
      ハーネス
    </RouterLink>
    <RouterLink
      to="/trash"
      active-class="bg-gray-100 font-medium"
      class="flex items-center gap-2 px-3 py-2 text-sm rounded mx-1 hover:bg-gray-100 text-gray-700"
    >
      <TrashIcon class="w-4 h-4 shrink-0" />
      ゴミ箱
      <span
        v-if="trashCount > 0"
        class="ml-auto text-xs bg-gray-200 text-gray-600 rounded-full px-1.5 py-0.5 leading-none"
      >
        {{ trashCount }}
      </span>
    </RouterLink>
  </nav>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { TrashIcon, ListBulletIcon } from "@heroicons/vue/24/outline";

const POLL_INTERVAL_MS = 2000;
const trashCount = ref(0);
let pollTimer: ReturnType<typeof setInterval> | null = null;

async function fetchCount() {
  const items = await invoke<{ harness_id: string }[]>("list_trash");
  trashCount.value = items.length;
}

onMounted(() => {
  void fetchCount();
  pollTimer = setInterval(fetchCount, POLL_INTERVAL_MS);
});
onUnmounted(() => {
  if (pollTimer) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
});
</script>
