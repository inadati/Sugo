<template>
  <div class="fixed inset-0 bg-black/30 flex items-center justify-center z-50">
    <div class="bg-white rounded-lg shadow-xl p-6 w-96">
      <h3 class="font-semibold text-lg mb-4">新規ハーネス</h3>
      <label class="block text-xs text-gray-500 mb-1">名前（必須）</label>
      <input
        data-testid="name"
        v-model="name"
        type="text"
        placeholder="例: しりとりデモ"
        autocomplete="off"
        autocorrect="off"
        autocapitalize="off"
        spellcheck="false"
        class="w-full border border-gray-300 rounded px-3 py-2 mb-3 focus:outline-none focus:border-blue-400"
        @keydown.enter="onNameEnter"
      />
      <label class="block text-xs text-gray-500 mb-1">説明（任意）</label>
      <textarea
        data-testid="desc"
        v-model="description"
        rows="3"
        placeholder="このハーネスの目的など"
        autocomplete="off"
        autocorrect="off"
        autocapitalize="off"
        spellcheck="false"
        class="w-full border border-gray-300 rounded px-3 py-2 mb-4 focus:outline-none focus:border-blue-400 resize-none"
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
        >作成</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

const emit = defineEmits<{ close: []; created: [harnessId: string] }>();

const name = ref("");
const description = ref("");
const errorMsg = ref("");
const submitting = ref(false);

function onNameEnter(e: KeyboardEvent) {
  // IME変換確定のEnter（isComposing）では送信しない。
  if (e.isComposing) return;
  void submit();
}

async function submit() {
  if (submitting.value) return;
  if (!name.value.trim()) {
    errorMsg.value = "名前を入力してください。";
    return;
  }
  submitting.value = true;
  errorMsg.value = "";
  try {
    const result = await invoke<{ harness_id: string }>("create_harness", {
      name: name.value.trim(),
      description: description.value.trim() || null,
    });
    emit("created", result.harness_id);
  } catch {
    errorMsg.value = "作成に失敗しました。";
  } finally {
    submitting.value = false;
  }
}
</script>
