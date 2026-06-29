<template>
  <div class="min-h-screen bg-gray-50">
    <header class="bg-white border-b border-gray-200 px-6 py-3 flex justify-center items-center">
      <h1 class="text-sm font-bold text-gray-800 cursor-pointer select-none" @click="router.push('/')">
        Sugo {{ version }}
      </h1>
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
