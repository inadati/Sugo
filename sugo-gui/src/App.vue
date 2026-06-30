<template>
  <div class="h-screen flex flex-col bg-gray-50 overflow-hidden">
    <header
      class="shrink-0 bg-white border-b border-gray-200 h-[28px] flex justify-center items-center select-none"
      data-tauri-drag-region
      @mousedown.left="startDrag"
    >
      <span class="text-xs font-bold text-gray-600 cursor-pointer" @click="router.push('/')">
        Sugo {{ version }}
      </span>
    </header>
    <main class="flex-1 overflow-hidden flex flex-col p-4">
      <RouterView />
    </main>
  </div>
</template>

<script setup lang="ts">
import { useRouter } from "vue-router";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { version } from "../package.json";

const router = useRouter();

function startDrag() {
  getCurrentWindow().startDragging().catch(() => {});
}
</script>
