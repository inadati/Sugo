<template>
  <!-- 背景クリックで閉じる透明レイヤー -->
  <div class="fixed inset-0 z-40" @click="$emit('close')" @contextmenu.prevent="$emit('close')" />
  <div
    class="fixed z-50 bg-white rounded shadow-lg border border-gray-200 py-1 w-48 text-sm"
    :style="{ left: `${x}px`, top: `${y}px` }"
  >
    <div class="px-3 py-1 text-[10px] font-semibold tracking-wide text-gray-400 uppercase">
      フォルダへ移動
    </div>
    <button
      data-testid="move-to-uncategorized"
      class="flex items-center justify-between w-full text-left px-3 py-1.5 hover:bg-gray-100 text-gray-700"
      @click="move(null)"
    >
      <span>未分類</span>
      <CheckIcon v-if="currentFolderId === null" class="w-3.5 h-3.5 text-blue-500" />
    </button>
    <button
      v-for="f in folders"
      :key="f.folder_id"
      :data-testid="`move-to-${f.folder_id}`"
      class="flex items-center justify-between w-full text-left px-3 py-1.5 hover:bg-gray-100 text-gray-700"
      @click="move(f.folder_id)"
    >
      <span class="truncate">{{ f.name }}</span>
      <CheckIcon v-if="currentFolderId === f.folder_id" class="w-3.5 h-3.5 text-blue-500 shrink-0" />
    </button>

    <div class="border-t border-gray-100 my-1" />

    <button
      data-testid="rename-from-menu"
      class="flex items-center gap-2 w-full text-left px-3 py-1.5 hover:bg-gray-100 text-gray-700"
      @click="$emit('rename', harnessId); $emit('close')"
    >
      <PencilIcon class="w-3.5 h-3.5" />
      名前を変更
    </button>

    <button
      data-testid="trash-from-menu"
      class="flex items-center gap-2 w-full text-left px-3 py-1.5 hover:bg-gray-100 text-red-500"
      @click="$emit('trash', harnessId); $emit('close')"
    >
      <TrashIcon class="w-3.5 h-3.5" />
      ゴミ箱へ移動
    </button>
  </div>
</template>

<script setup lang="ts">
// ハーネス行の右クリックで開くコンテキストメニュー。
// 「フォルダへ移動」（フォルダ一覧＋未分類）と「ゴミ箱へ移動」を提示する。
import { CheckIcon, PencilIcon, TrashIcon } from "@heroicons/vue/24/outline";

interface FolderSummary {
  folder_id: string;
  name: string;
  harness_count: number;
}

const props = defineProps<{
  x: number;
  y: number;
  harnessId: string;
  currentFolderId: string | null;
  folders: FolderSummary[];
}>();

const emit = defineEmits<{
  move: [payload: { harnessId: string; folderId: string | null }];
  rename: [harnessId: string];
  trash: [harnessId: string];
  close: [];
}>();

function move(folderId: string | null) {
  emit("move", { harnessId: props.harnessId, folderId });
  emit("close");
}
</script>
