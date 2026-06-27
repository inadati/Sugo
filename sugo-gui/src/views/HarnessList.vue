<template>
  <div>
    <h2 class="text-lg font-semibold mb-4">ハーネス一覧</h2>
    <ul class="space-y-2">
      <li
        v-for="h in harnesses"
        :key="h.harness_id"
        class="bg-white rounded border border-gray-200 px-4 py-3 cursor-pointer hover:bg-gray-50 flex items-center justify-between"
        @click="router.push(`/harness/${h.harness_id}`)"
      >
        <span class="font-medium">{{ h.name }}</span>
        <div class="flex items-center gap-2">
          <span class="text-sm text-gray-400">v{{ h.current_version }}</span>
          <span v-if="h.has_draft" class="text-xs bg-yellow-100 text-yellow-800 px-2 py-0.5 rounded font-bold">
            DRAFT
          </span>
        </div>
      </li>
    </ul>
    <p v-if="harnesses.length === 0" class="text-gray-400">ハーネスがありません。MCP の sugo_create_harness で作成してください。</p>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useRouter } from "vue-router";

interface HarnessSummary {
  harness_id: string;
  name: string;
  current_version: number;
  has_draft: boolean;
}

const router = useRouter();
const harnesses = ref<HarnessSummary[]>([]);

onMounted(async () => {
  harnesses.value = await invoke<HarnessSummary[]>("list_harnesses");
});
</script>
