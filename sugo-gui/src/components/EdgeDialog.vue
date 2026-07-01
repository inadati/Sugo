<template>
  <div class="fixed inset-0 bg-black/30 flex items-center justify-center z-50">
    <div class="bg-white rounded-lg shadow-xl p-6 w-96">
      <h3 class="font-semibold text-lg mb-1">エッジを追加</h3>
      <p class="text-sm text-gray-500 mb-4">
        <span class="font-medium text-gray-700">{{ fromName }}</span>
        <span class="mx-1">→</span>
        <span class="font-medium text-gray-700">{{ toName }}</span>
      </p>

      <label class="block text-xs text-gray-500 mb-1">ラベル（必須）</label>
      <input
        data-testid="edge-label"
        v-model="label"
        type="text"
        placeholder="例: 次へ"
        class="w-full border border-gray-300 rounded px-3 py-2 mb-3 focus:outline-none focus:border-blue-400"
        @keydown.enter="submit"
      />

      <label class="block text-xs text-gray-500 mb-1">ガード条件（任意）</label>
      <input
        data-testid="edge-guard"
        v-model="guard"
        type="text"
        placeholder="例: 続ける"
        class="w-full border border-gray-300 rounded px-3 py-2 mb-4 focus:outline-none focus:border-blue-400"
        @keydown.enter="submit"
      />

      <p v-if="errorMsg" class="text-red-500 text-sm mb-3">{{ errorMsg }}</p>
      <div class="flex justify-end gap-2">
        <button
          data-testid="edge-cancel"
          class="px-4 py-2 text-gray-600 hover:bg-gray-100 rounded"
          @click="$emit('close')"
        >キャンセル</button>
        <button
          data-testid="edge-submit"
          class="px-4 py-2 bg-blue-500 text-white rounded hover:bg-blue-600 disabled:opacity-50"
          :disabled="submitting"
          @click="submit"
        >追加</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

const props = defineProps<{
  harnessId: string;
  from: string;
  to: string;
  fromName: string;
  toName: string;
  lockVersion: number;
}>();
const emit = defineEmits<{
  close: [];
  added: [newVersion: number, lockVersion: number];
}>();

const label = ref("");
const guard = ref("");
const errorMsg = ref("");
const submitting = ref(false);

async function submit() {
  if (!label.value.trim()) {
    errorMsg.value = "ラベルを入力してください。";
    return;
  }
  submitting.value = true;
  errorMsg.value = "";
  try {
    const result = await invoke<{ new_version: number; lock_version: number }>("add_edge", {
      harnessId: props.harnessId,
      from: props.from,
      to: props.to,
      label: label.value.trim(),
      guard: guard.value.trim() || null,
      lockVersion: props.lockVersion,
    });
    emit("added", result.new_version, result.lock_version);
  } catch (e) {
    const msg = String(e);
    if (msg.includes("lock_conflict")) {
      errorMsg.value = "他で編集が入りました。再読み込みしてください。";
    } else if (msg.includes("duplicate_edge")) {
      errorMsg.value = "同じ経路・ラベルのエッジが既に存在します。";
    } else if (msg.includes("empty_label")) {
      errorMsg.value = "ラベルを入力してください。";
    } else {
      errorMsg.value = "エラーが発生しました。";
    }
  } finally {
    submitting.value = false;
  }
}
</script>
