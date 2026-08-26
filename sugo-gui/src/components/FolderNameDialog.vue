<template>
  <div class="fixed inset-0 bg-black/30 flex items-center justify-center z-50">
    <div class="bg-white rounded-lg shadow-xl p-6 w-96">
      <h3 class="font-semibold text-lg mb-4">
        {{ mode === "create" ? "新規フォルダ" : "フォルダ名を変更" }}
      </h3>
      <label class="block text-xs text-gray-500 mb-1">フォルダ名</label>
      <input
        data-testid="folder-name"
        v-model="name"
        type="text"
        placeholder="例: 開発"
        autocomplete="off"
        autocorrect="off"
        autocapitalize="off"
        spellcheck="false"
        class="w-full border border-gray-300 rounded px-3 py-2 mb-4 focus:outline-none focus:border-blue-400"
      />
      <p v-if="errorMsg" class="text-red-500 text-sm mb-3">{{ errorMsg }}</p>
      <div class="flex justify-end gap-2">
        <button
          data-testid="folder-cancel"
          class="px-4 py-2 text-gray-600 hover:bg-gray-100 rounded"
          @click="$emit('close')"
        >キャンセル</button>
        <button
          data-testid="folder-submit"
          class="px-4 py-2 bg-blue-500 text-white rounded hover:bg-blue-600 disabled:opacity-50"
          :disabled="submitting"
          @click="submit"
        >保存</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
// フォルダの新規作成・改名で共用する入力ダイアログ。
//
// UI 規律: Enter キーによる確定は実装しない（IME の変換確定と衝突するため）。
// 保存はボタンのクリックのみで行う。<input> に @keydown.enter のハンドラを
// 意図的に付けていない。
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

const props = defineProps<{
  mode: "create" | "rename";
  folderId?: string;
  initialName?: string;
}>();
const emit = defineEmits<{ close: []; saved: [] }>();

const name = ref(props.initialName ?? "");
const errorMsg = ref("");
const submitting = ref(false);

async function submit() {
  if (submitting.value) return;
  const trimmed = name.value.trim();
  if (!trimmed) {
    errorMsg.value = "フォルダ名を入力してください。";
    return;
  }
  submitting.value = true;
  errorMsg.value = "";
  try {
    if (props.mode === "create") {
      await invoke("create_folder", { name: trimmed });
    } else {
      await invoke("rename_folder", { folderId: props.folderId, name: trimmed });
    }
    emit("saved");
  } catch (e: unknown) {
    errorMsg.value = String(e);
  } finally {
    submitting.value = false;
  }
}
</script>
