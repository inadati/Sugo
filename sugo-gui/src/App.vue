<template>
  <div class="min-h-screen bg-gray-50">
    <header
      class="bg-white border-b border-gray-200 h-[28px] flex justify-center items-center select-none"
      data-tauri-drag-region
    >
      <span class="text-xs font-bold text-gray-600 cursor-pointer" @click="router.push('/')">
        Sugo {{ version }}
      </span>
    </header>
    <main class="p-6">
      <RouterView />
    </main>
  </div>
</template>

<script setup lang="ts">
import { useRouter } from "vue-router";
import { version } from "../package.json";
const router = useRouter();
</script>
