<template>
  <nav class="w-[200px] shrink-0 bg-white border-r border-gray-200 flex flex-col py-2 gap-0.5">
    <RouterLink
      to="/"
      exact-active-class="bg-gray-100 font-medium"
      class="flex items-center gap-2 px-3 py-2 text-sm rounded mx-1 hover:bg-gray-100 text-gray-700"
    >
      <ListBulletIcon class="w-4 h-4 shrink-0" />
      すべてのハーネス
    </RouterLink>
    <RouterLink
      to="/uncategorized"
      data-testid="folder-drop-uncategorized"
      active-class="bg-gray-100 font-medium"
      class="flex items-center gap-2 px-3 py-2 text-sm rounded mx-1 hover:bg-gray-100 text-gray-700"
      :class="{ 'bg-blue-50 ring-1 ring-blue-300': dragOverId === UNCATEGORIZED }"
      @dragover.prevent="dragOverId = UNCATEGORIZED"
      @dragleave="dragOverId = null"
      @drop.prevent="onDrop($event, null)"
    >
      <InboxIcon class="w-4 h-4 shrink-0" />
      未分類
    </RouterLink>

    <div class="border-t border-gray-100 my-1 mx-2" />

    <div class="flex items-center justify-between px-3 pt-1 pb-0.5">
      <span class="text-[10px] font-semibold tracking-wide text-gray-400 uppercase">フォルダ</span>
    </div>
    <RouterLink
      v-for="f in folders"
      :key="f.folder_id"
      :to="`/folder/${f.folder_id}`"
      :data-testid="`folder-drop-${f.folder_id}`"
      active-class="bg-gray-100 font-medium"
      class="group flex items-center gap-2 px-3 py-2 text-sm rounded mx-1 hover:bg-gray-100 text-gray-700"
      :class="{ 'bg-blue-50 ring-1 ring-blue-300': dragOverId === f.folder_id }"
      @dragover.prevent="dragOverId = f.folder_id"
      @dragleave="dragOverId = null"
      @drop.prevent="onDrop($event, f.folder_id)"
    >
      <FolderIcon class="w-4 h-4 shrink-0" />
      <span class="truncate flex-1">{{ f.name }}</span>
      <span class="text-xs bg-gray-200 text-gray-600 rounded-full px-1.5 py-0.5 leading-none">
        {{ f.harness_count }}
      </span>
      <button
        data-testid="rename-folder-btn"
        class="opacity-0 group-hover:opacity-100 p-0.5 text-gray-400 hover:text-blue-500 transition-opacity"
        @click.stop.prevent="openRename(f)"
      >
        <PencilIcon class="w-3.5 h-3.5" />
      </button>
      <button
        data-testid="delete-folder-btn"
        class="opacity-0 group-hover:opacity-100 p-0.5 text-gray-400 hover:text-red-500 transition-opacity"
        @click.stop.prevent="doDeleteFolder(f)"
      >
        <TrashIcon class="w-3.5 h-3.5" />
      </button>
    </RouterLink>
    <button
      data-testid="new-folder-btn"
      class="flex items-center gap-2 px-3 py-2 text-sm rounded mx-1 text-gray-500 hover:bg-gray-100 hover:text-gray-700 text-left"
      @click="showCreateDialog = true"
    >
      ＋新規フォルダ
    </button>

    <div class="border-t border-gray-100 my-1 mx-2" />

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

    <FolderNameDialog
      v-if="showCreateDialog"
      mode="create"
      @close="showCreateDialog = false"
      @saved="showCreateDialog = false; fetchFolders()"
    />
    <FolderNameDialog
      v-if="renameTarget"
      mode="rename"
      :folder-id="renameTarget.folder_id"
      :initial-name="renameTarget.name"
      @close="renameTarget = null"
      @saved="renameTarget = null; fetchFolders()"
    />

    <!-- トースト -->
    <div
      v-if="toast"
      data-testid="toast"
      class="fixed top-4 left-1/2 -translate-x-1/2 z-50 bg-gray-800 text-white text-sm px-4 py-2 rounded shadow"
    >{{ toast }}</div>
  </nav>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useRoute, useRouter } from "vue-router";
import { TrashIcon, ListBulletIcon, InboxIcon, FolderIcon, PencilIcon } from "@heroicons/vue/24/outline";
import FolderNameDialog from "./FolderNameDialog.vue";
import { useToast } from "../composables/useToast";

interface FolderSummary {
  folder_id: string;
  name: string;
  harness_count: number;
}

// ドロップ先の識別に使う「未分類」の擬似 id（実フォルダの id と衝突しない）。
const UNCATEGORIZED = "__uncategorized__";

const POLL_INTERVAL_MS = 2000;
const route = useRoute();
const router = useRouter();
const { toast, showToast } = useToast();
const trashCount = ref(0);
const folders = ref<FolderSummary[]>([]);
const showCreateDialog = ref(false);
const renameTarget = ref<FolderSummary | null>(null);
const dragOverId = ref<string | null>(null);
let pollTimer: ReturnType<typeof setInterval> | null = null;

/**
 * invoke() の reject メッセージから NotFound 系（存在しない folder_id、
 * 削除済みフォルダへの移動）を判定する。design.md のエラー処理表の通り、
 * NotFound はトースト通知＋一覧再取得、それ以外（DB障害等）はエラー表示のみで
 * 状態は変更しない。
 */
function isNotFoundError(e: unknown): boolean {
  return String(e).includes("not found");
}

async function fetchTrashCount() {
  const items = await invoke<{ harness_id: string }[]>("list_trash");
  trashCount.value = items.length;
}

async function fetchFolders() {
  folders.value = await invoke<FolderSummary[]>("list_folders");
}

function openRename(f: FolderSummary) {
  renameTarget.value = f;
}

async function doDeleteFolder(f: FolderSummary) {
  const wasViewingDeletedFolder = route.path === `/folder/${f.folder_id}`;
  try {
    await invoke("delete_folder", { folderId: f.folder_id });
    await fetchFolders();
    // design.md L137: 削除中のフォルダを表示していた場合はすべてのハーネスへ遷移する
    if (wasViewingDeletedFolder) {
      router.push("/");
    }
  } catch (e) {
    if (isNotFoundError(e)) {
      showToast("フォルダは既に見つかりません。一覧を更新しました。");
      await fetchFolders();
    } else {
      showToast("フォルダの削除に失敗しました。");
    }
  }
}

async function onDrop(e: DragEvent, folderId: string | null) {
  dragOverId.value = null;
  const harnessId = e.dataTransfer?.getData("text/plain");
  if (!harnessId) return;
  try {
    await invoke("move_harness_to_folder", { harnessId, folderId });
    await fetchFolders();
  } catch (e) {
    if (isNotFoundError(e)) {
      showToast("移動先が見つかりません。一覧を更新しました。");
      await fetchFolders();
    } else {
      showToast("ハーネスの移動に失敗しました。");
    }
  }
}

onMounted(() => {
  void fetchTrashCount();
  void fetchFolders();
  pollTimer = setInterval(() => {
    void fetchTrashCount();
    void fetchFolders();
  }, POLL_INTERVAL_MS);
});
onUnmounted(() => {
  if (pollTimer) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
});
</script>
