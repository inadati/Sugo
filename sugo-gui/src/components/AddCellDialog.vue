<template>
  <div class="fixed inset-0 bg-black/30 flex items-center justify-center z-50">
    <div class="bg-white rounded-lg shadow-xl p-6 w-80">
      <h3 class="font-semibold text-lg mb-4">マスを追加</h3>
      <input
        v-model="cellName"
        type="text"
        placeholder="マス名（必須）"
        class="w-full border border-gray-300 rounded px-3 py-2 mb-4 focus:outline-none focus:border-blue-400"
        @keydown.enter="submit"
      />
      <p v-if="errorMsg" class="text-red-500 text-sm mb-3">{{ errorMsg }}</p>
      <div class="flex justify-end gap-2">
        <button
          data-testid="cancel"
          class="px-4 py-2 text-gray-600 hover:bg-gray-100 rounded"
          @click="$emit('close')"
        >キャンセル</button>
        <button
          data-testid="submit"
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

const props = defineProps<{ harnessId: string; lockVersion: number }>();
const emit = defineEmits<{ close: []; added: [newVersion: number, lockVersion: number] }>();

const cellName = ref("");
const errorMsg = ref("");
const submitting = ref(false);

async function submit() {
  if (!cellName.value.trim()) return;
  submitting.value = true;
  errorMsg.value = "";
  try {
    const result = await invoke<{ new_version: number; lock_version: number }>("add_cell", {
      harnessId: props.harnessId,
      cellName: cellName.value.trim(),
      lockVersion: props.lockVersion,
    });
    emit("added", result.new_version, result.lock_version);
  } catch (e) {
    const msg = String(e);
    if (msg.includes("lock_conflict")) {
      errorMsg.value = "他で編集が入りました。再読み込みしてください。";
    } else {
      errorMsg.value = "エラーが発生しました。";
    }
  } finally {
    submitting.value = false;
  }
}
</script>
