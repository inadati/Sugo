<template>
  <div class="fixed inset-0 bg-black/30 flex items-center justify-center z-50">
    <div class="bg-white rounded-lg shadow-xl p-6 w-96">
      <h3 class="font-semibold text-lg mb-4">{{ title }}</h3>
      <label class="block text-xs text-gray-500 mb-1">{{ fieldLabel }}</label>
      <input
        data-testid="folder-name"
        v-model="name"
        type="text"
        :placeholder="placeholder"
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
import { ref, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";

const props = withDefaults(
  defineProps<{
    mode: "create" | "rename";
    /** 対象の種別。省略時はフォルダ（既存の呼び出し側との互換のため）。 */
    entity?: "folder" | "harness";
    folderId?: string;
    harnessId?: string;
    initialName?: string;
  }>(),
  { entity: "folder" },
);
const emit = defineEmits<{ close: []; saved: []; "not-found": [] }>();

const name = ref(props.initialName ?? "");
const errorMsg = ref("");
const submitting = ref(false);

const isHarness = computed(() => props.entity === "harness");
const noun = computed(() => (isHarness.value ? "ハーネス名" : "フォルダ名"));
const fieldLabel = computed(() => noun.value);
const title = computed(() =>
  props.mode === "create" ? `新規${isHarness.value ? "ハーネス" : "フォルダ"}` : `${noun.value}を変更`,
);
const placeholder = computed(() => (isHarness.value ? "例: 記事づくり" : "例: 開発"));

/**
 * invoke() の reject メッセージから NotFound（存在しない folder_id、改名中に
 * 別クライアント/MCPから削除された等）を判定する。AppSidebar.vue の
 * isNotFoundError と同じ基準（"not found" の部分一致）で判定する。
 * design.md のエラー処理表どおり、NotFound はダイアログ内表示ではなく
 * 呼び出し元のトースト通知＋一覧再取得に振り分ける（Validation/Conflict は
 * 引き続きダイアログ内表示のまま維持する）。
 */
function isNotFoundError(e: unknown): boolean {
  return String(e).includes("not found");
}

async function submit() {
  if (submitting.value) return;
  const trimmed = name.value.trim();
  if (!trimmed) {
    errorMsg.value = `${noun.value}を入力してください。`;
    return;
  }
  submitting.value = true;
  errorMsg.value = "";
  try {
    if (isHarness.value) {
      await invoke("rename_harness", { harnessId: props.harnessId, name: trimmed });
    } else if (props.mode === "create") {
      await invoke("create_folder", { name: trimmed });
    } else {
      await invoke("rename_folder", { folderId: props.folderId, name: trimmed });
    }
    emit("saved");
  } catch (e: unknown) {
    if (isNotFoundError(e)) {
      emit("not-found");
      return;
    }
    errorMsg.value = String(e);
  } finally {
    submitting.value = false;
  }
}
</script>
