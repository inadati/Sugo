<template>
  <div class="min-h-screen bg-gray-50">
    <header class="bg-white border-b border-gray-200 px-6 py-3">
      <h1 class="text-xl font-bold text-gray-800 cursor-pointer" @click="router.push('/')">
        Sugo
      </h1>
    </header>
    <main class="p-6">
      <RouterView />
    </main>
  </div>
</template>

<script setup lang="ts">
import { useRouter } from "vue-router";
const router = useRouter();
</script>
