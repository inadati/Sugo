<template>
  <div class="px-6 py-5">
    <div class="flex items-center justify-between mb-5">
      <h2 class="text-lg font-semibold">{{ scopeTitle }}</h2>
      <button
        data-testid="create-harness-btn"
        class="bg-blue-500 text-white px-3 py-1.5 rounded text-sm hover:bg-blue-600"
        @click="showCreate = true"
      >＋新規ハーネス</button>
    </div>
    <ul class="space-y-2">
      <li
        v-for="h in visible"
        :key="h.harness_id"
        data-testid="harness-row"
        draggable="true"
        class="group bg-white rounded border border-gray-200 px-4 py-3 cursor-pointer hover:bg-gray-50 flex items-center justify-between"
        @click="router.push(`/harness/${h.harness_id}`)"
        @dragstart="onDragStart($event, h)"
        @contextmenu.prevent="openContextMenu($event, h)"
      >
        <span class="font-medium">{{ h.name }}</span>
        <div class="flex items-center gap-2">
          <span class="text-sm text-gray-400">v{{ h.current_version }}</span>
          <span
            v-if="h.has_draft"
            class="text-xs bg-yellow-100 text-yellow-800 px-2 py-0.5 rounded font-bold"
          >
            DRAFT
          </span>
          <button
            data-testid="trash-btn"
            class="opacity-0 group-hover:opacity-100 p-1 text-gray-400 hover:text-red-500 transition-opacity"
            @click.stop="confirmTrash(h)"
          >
            <TrashIcon class="w-4 h-4" />
          </button>
        </div>
      </li>
    </ul>
    <p v-if="visible.length === 0" class="text-gray-400">
      {{ emptyMessage }}
    </p>

    <!-- 確認ダイアログ -->
    <div
      v-if="trashTarget"
      data-testid="trash-dialog"
      class="fixed inset-0 bg-black/30 z-50 flex items-center justify-center"
    >
      <div class="bg-white rounded-lg shadow-lg p-6 w-80">
        <p class="text-sm font-medium mb-4">
          "{{ trashTarget.name }}" をゴミ箱に移動しますか？
        </p>
        <p v-if="trashError" class="text-xs text-red-500 mb-3">{{ trashError }}</p>
        <div class="flex gap-2 justify-end">
          <button
            data-testid="trash-cancel-btn"
            class="px-3 py-1.5 text-sm text-gray-600 hover:text-gray-900"
            @click="trashTarget = null; trashError = null"
          >
            キャンセル
          </button>
          <button
            class="px-3 py-1.5 text-sm bg-red-500 text-white rounded hover:bg-red-600"
            @click="doTrash"
          >
            移動する
          </button>
        </div>
      </div>
    </div>

    <!-- 新規作成ダイアログ -->
    <NewHarnessDialog
      v-if="showCreate"
      @close="showCreate = false"
      @created="onCreated"
    />

    <!-- 右クリックコンテキストメニュー -->
    <HarnessContextMenu
      v-if="contextMenu"
      :x="contextMenu.x"
      :y="contextMenu.y"
      :harness-id="contextMenu.harnessId"
      :current-folder-id="contextMenu.currentFolderId"
      :folders="folders"
      @move="onMoveFromMenu"
      @trash="onTrashFromMenu"
      @close="contextMenu = null"
    />

    <!-- トースト -->
    <div
      v-if="toast"
      data-testid="toast"
      class="fixed top-4 left-1/2 -translate-x-1/2 z-50 bg-gray-800 text-white text-sm px-4 py-2 rounded shadow"
    >{{ toast }}</div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useRoute, useRouter } from "vue-router";
import { TrashIcon } from "@heroicons/vue/24/outline";
import NewHarnessDialog from "../components/NewHarnessDialog.vue";
import HarnessContextMenu from "../components/HarnessContextMenu.vue";
import { useToast } from "../composables/useToast";

interface HarnessSummary {
  harness_id: string;
  name: string;
  current_version: number;
  has_draft: boolean;
  folder_id: string | null;
  folder_name: string | null;
}

interface FolderSummary {
  folder_id: string;
  name: string;
  harness_count: number;
}

interface ContextMenuState {
  x: number;
  y: number;
  harnessId: string;
  currentFolderId: string | null;
}

const POLL_INTERVAL_MS = 2000;
const router = useRouter();
const route = useRoute();
const { toast, showToast } = useToast();
const harnesses = ref<HarnessSummary[]>([]);
const folders = ref<FolderSummary[]>([]);
const trashTarget = ref<HarnessSummary | null>(null);
const trashError = ref<string | null>(null);
const showCreate = ref(false);
const contextMenu = ref<ContextMenuState | null>(null);
let pollTimer: ReturnType<typeof setInterval> | null = null;

// 現在のルートから表示スコープを導出する。
// "all" = すべてのハーネス、"uncategorized" = 未分類、"folder" = 特定フォルダ。
const scope = computed<{ kind: "all" | "uncategorized" | "folder"; folderId?: string }>(() => {
  if (route.path === "/uncategorized") return { kind: "uncategorized" };
  if (route.params.id) return { kind: "folder", folderId: String(route.params.id) };
  return { kind: "all" };
});

const visible = computed(() => {
  const s = scope.value;
  if (s.kind === "all") return harnesses.value;
  if (s.kind === "uncategorized") return harnesses.value.filter((h) => !h.folder_id);
  return harnesses.value.filter((h) => h.folder_id === s.folderId);
});

const scopeTitle = computed(() => {
  const s = scope.value;
  if (s.kind === "uncategorized") return "未分類";
  if (s.kind === "folder") {
    const f = folders.value.find((f) => f.folder_id === s.folderId);
    return f?.name ?? "";
  }
  return "ハーネス一覧";
});

const emptyMessage = computed(() => {
  const s = scope.value;
  if (s.kind === "uncategorized") return "未分類のハーネスはありません。";
  if (s.kind === "folder") return "このフォルダにはまだハーネスがありません。";
  return "まだハーネスがありません。「＋新規ハーネス」から作成してください。";
});

async function fetchHarnesses() {
  harnesses.value = await invoke<HarnessSummary[]>("list_harnesses");
}

async function fetchFolders() {
  folders.value = await invoke<FolderSummary[]>("list_folders");
}

function onCreated(harnessId: string) {
  showCreate.value = false;
  router.push(`/harness/${harnessId}`);
}

function confirmTrash(h: HarnessSummary) {
  trashTarget.value = h;
  trashError.value = null;
}

async function doTrash() {
  if (!trashTarget.value) return;
  try {
    await invoke("trash_harness", { harnessId: trashTarget.value.harness_id });
    trashTarget.value = null;
    await fetchHarnesses();
  } catch (e: unknown) {
    trashError.value =
      String(e).includes("active_run")
        ? "実行中のRunがあるため移動できません"
        : String(e);
  }
}

function onDragStart(e: DragEvent, h: HarnessSummary) {
  e.dataTransfer?.setData("text/plain", h.harness_id);
}

function openContextMenu(e: MouseEvent, h: HarnessSummary) {
  contextMenu.value = {
    x: e.clientX,
    y: e.clientY,
    harnessId: h.harness_id,
    currentFolderId: h.folder_id,
  };
}

async function onMoveFromMenu(payload: { harnessId: string; folderId: string | null }) {
  contextMenu.value = null;
  try {
    await invoke("move_harness_to_folder", { harnessId: payload.harnessId, folderId: payload.folderId });
    await Promise.all([fetchHarnesses(), fetchFolders()]);
  } catch (e) {
    if (String(e).includes("not found")) {
      showToast("移動先が見つかりません。一覧を更新しました。");
      await Promise.all([fetchHarnesses(), fetchFolders()]);
    } else {
      showToast("ハーネスの移動に失敗しました。");
    }
  }
}

function onTrashFromMenu(harnessId: string) {
  contextMenu.value = null;
  const target = harnesses.value.find((h) => h.harness_id === harnessId);
  if (target) confirmTrash(target);
}

onMounted(() => {
  void fetchHarnesses();
  void fetchFolders();
  pollTimer = setInterval(() => {
    void fetchHarnesses();
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
