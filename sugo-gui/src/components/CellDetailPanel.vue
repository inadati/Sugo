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
    <input
      data-testid="name-input"
      v-model="nameDraft"
      type="text"
      autocomplete="off"
      autocorrect="off"
      autocapitalize="off"
      spellcheck="false"
      class="w-full border border-gray-300 rounded px-2 py-1 mb-3 focus:outline-none focus:border-blue-400"
      @blur="save"
    />

    <!-- メタ情報 -->
    <dl class="text-sm text-gray-600 mb-4 space-y-1">
      <div><dt class="inline text-gray-400">id: </dt><dd class="inline">{{ cell.id }}</dd></div>
      <div><dt class="inline text-gray-400">status: </dt><dd class="inline">{{ cell.status }}</dd></div>
      <div><dt class="inline text-gray-400">terminal: </dt><dd class="inline">{{ cell.terminal }}</dd></div>
    </dl>

    <!-- プロンプト閲覧 -->
    <label class="block text-xs text-gray-500 mb-1">プロンプト</label>
    <pre class="text-sm bg-gray-50 border border-gray-200 rounded p-3 whitespace-pre-wrap break-words">{{ cell.prompt || "（未登録）" }}</pre>

    <!-- 要望メモ -->
    <label class="block text-xs text-gray-500 mb-1 mt-4">AIへの要望メモ</label>
    <textarea
      data-testid="memo-input"
      v-model="memoDraft"
      rows="3"
      placeholder="このマスのプロンプトをこう直してほしい、など"
      autocomplete="off"
      autocorrect="off"
      autocapitalize="off"
      spellcheck="false"
      class="w-full border border-gray-300 rounded px-2 py-1 mb-3 focus:outline-none focus:border-blue-400 resize-none"
      @blur="save"
    />
    <p v-if="errorMsg" data-testid="save-error" class="text-red-500 text-sm mt-1">{{ errorMsg }}</p>

    <!-- マス削除（START 以外は draft/active を問わず削除可） -->
    <div v-if="!isStart" class="mt-6 border-t border-gray-100 pt-4">
      <button
        data-testid="cell-delete"
        class="w-full px-3 py-2 bg-red-50 text-red-600 border border-red-200 rounded hover:bg-red-100 disabled:opacity-50 text-sm"
        :disabled="deleting"
        @click="deleteCell"
      >このマスを削除</button>
      <p v-if="deleteErrorMsg" class="text-red-500 text-sm mt-1">{{ deleteErrorMsg }}</p>
    </div>
    <p v-else class="mt-6 border-t border-gray-100 pt-4 text-xs text-gray-400">START マスは削除できません。</p>
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
  memo: string;
}

const props = defineProps<{ harnessId: string; cell: CellData; lockVersion: number; isStart?: boolean }>();
const emit = defineEmits<{
  close: [];
  renamed: [newVersion: number, lockVersion: number];
  deleted: [newVersion: number, lockVersion: number];
}>();

const nameDraft = ref(props.cell.name);
const errorMsg = ref("");
const saving = ref(false);
const deleting = ref(false);
const deleteErrorMsg = ref("");
const memoDraft = ref(props.cell.memo);

watch(() => props.cell.id, () => {
  nameDraft.value = props.cell.name;
  errorMsg.value = "";
  deleteErrorMsg.value = "";
  memoDraft.value = props.cell.memo;
});

async function save() {
  // タイトル欄・メモ欄いずれかからフォーカスが外れたタイミングで自動保存する。
  // 二重発火（例: name-input → memo-input のタブ移動でblurが連続する）を防ぐ。
  if (saving.value) return;
  if (nameDraft.value === props.cell.name && memoDraft.value === props.cell.memo) return;
  if (!nameDraft.value.trim()) {
    errorMsg.value = "タイトルを入力してください。";
    return;
  }
  saving.value = true;
  errorMsg.value = "";
  try {
    const renamed = await invoke<{ new_version: number; lock_version: number }>("rename_cell", {
      harnessId: props.harnessId,
      cellId: props.cell.id,
      newName: nameDraft.value.trim(),
      lockVersion: props.lockVersion,
    });
    const memoSaved = await invoke<{ new_version: number; lock_version: number }>("set_cell_memo", {
      harnessId: props.harnessId,
      cellId: props.cell.id,
      memo: memoDraft.value,
      lockVersion: renamed.lock_version,
    });
    emit("renamed", memoSaved.new_version, memoSaved.lock_version);
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

async function deleteCell() {
  deleting.value = true;
  deleteErrorMsg.value = "";
  try {
    const result = await invoke<{ new_version: number; lock_version: number }>("delete_cell", {
      harnessId: props.harnessId,
      cellId: props.cell.id,
      lockVersion: props.lockVersion,
    });
    emit("deleted", result.new_version, result.lock_version);
  } catch (e) {
    const msg = String(e);
    if (msg.includes("lock_conflict")) {
      deleteErrorMsg.value = "他で編集が入りました。再読み込みしてください。";
    } else if (msg.includes("cannot_delete_start")) {
      deleteErrorMsg.value = "START マスは削除できません。";
    } else {
      deleteErrorMsg.value = "削除に失敗しました。";
    }
  } finally {
    deleting.value = false;
  }
}

</script>
