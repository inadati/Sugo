<template>
  <div class="fixed top-0 right-0 h-full w-96 bg-white border-l border-gray-200 shadow-xl p-5 z-40 overflow-y-auto">
    <div class="flex items-center justify-between mb-4">
      <h3 class="font-semibold text-lg">マスの詳細</h3>
      <button
        data-testid="panel-close"
        class="text-gray-400 hover:text-gray-600"
        @click="$emit('close')"
      >✕</button>
    </div>

    <!-- タイトル編集 -->
    <label class="block text-xs text-gray-500 mb-1">タイトル</label>
    <div class="flex gap-2 mb-1">
      <input
        data-testid="name-input"
        v-model="nameDraft"
        type="text"
        class="flex-1 border border-gray-300 rounded px-2 py-1 focus:outline-none focus:border-blue-400"
        @keydown.enter="save"
      />
      <button
        data-testid="name-save"
        class="px-3 py-1 bg-blue-500 text-white rounded hover:bg-blue-600 disabled:opacity-50"
        :disabled="saving"
        @click="save"
      >保存</button>
    </div>
    <p v-if="errorMsg" class="text-red-500 text-sm mb-3">{{ errorMsg }}</p>

    <!-- メタ情報 -->
    <dl class="text-sm text-gray-600 mt-4 mb-4 space-y-1">
      <div><dt class="inline text-gray-400">id: </dt><dd class="inline">{{ cell.id }}</dd></div>
      <div><dt class="inline text-gray-400">status: </dt><dd class="inline">{{ cell.status }}</dd></div>
      <div><dt class="inline text-gray-400">terminal: </dt><dd class="inline">{{ cell.terminal }}</dd></div>
    </dl>

    <!-- プロンプト閲覧 -->
    <label class="block text-xs text-gray-500 mb-1">プロンプト</label>
    <pre class="text-sm bg-gray-50 border border-gray-200 rounded p-3 whitespace-pre-wrap break-words">{{ cell.prompt || "（未登録）" }}</pre>
  </div>
</template>

<script setup lang="ts">
import { ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";

interface CellData {
  id: string;
  name: string;
  prompt: string;
  status: string;
  terminal: boolean;
}

const props = defineProps<{ harnessId: string; cell: CellData; lockVersion: number }>();
const emit = defineEmits<{ close: []; renamed: [newVersion: number, lockVersion: number] }>();

const nameDraft = ref(props.cell.name);
const errorMsg = ref("");
const saving = ref(false);

// 選択セルが切り替わったら下書きを同期する
watch(() => props.cell, (c) => { nameDraft.value = c.name; errorMsg.value = ""; });

async function save() {
  if (!nameDraft.value.trim()) return;
  saving.value = true;
  errorMsg.value = "";
  try {
    const result = await invoke<{ new_version: number; lock_version: number }>("rename_cell", {
      harnessId: props.harnessId,
      cellId: props.cell.id,
      newName: nameDraft.value.trim(),
      lockVersion: props.lockVersion,
    });
    emit("renamed", result.new_version, result.lock_version);
  } catch (e) {
    const msg = String(e);
    if (msg.includes("lock_conflict")) {
      errorMsg.value = "他で編集が入りました。再読み込みしてください。";
    } else if (msg.includes("empty_name")) {
      errorMsg.value = "タイトルを入力してください。";
    } else {
      errorMsg.value = "エラーが発生しました。";
    }
  } finally {
    saving.value = false;
  }
}
</script>
