<template>
  <div class="flex h-full overflow-hidden">
    <AppSidebar />
    <div class="flex-1 overflow-auto p-4">
      <RouterView />
    </div>
  </div>
</template>

<script setup lang="ts">
import AppSidebar from "../components/AppSidebar.vue";
</script>
