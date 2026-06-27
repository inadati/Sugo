<template>
  <div
    :class="[
      'rounded px-3 py-2 text-sm min-w-[120px] text-center border-2',
      data.status === 'draft' ? 'bg-yellow-50 border-yellow-400' : 'bg-white border-gray-300',
      data.isStart ? 'border-green-500' : '',
      data.terminal ? 'ring-2 ring-gray-400 ring-offset-1' : '',
    ]"
  >
    <div class="font-medium text-gray-800">{{ data.name }}</div>
    <div class="flex justify-center gap-1 mt-1">
      <span v-if="data.isStart" class="text-xs text-green-600 font-bold">START</span>
      <span v-if="data.terminal" class="text-xs text-gray-500 font-bold">END</span>
      <span v-if="data.status === 'draft'" class="text-xs text-yellow-700">draft</span>
    </div>
    <!-- Vue Flow のハンドル -->
    <Handle type="target" :position="Position.Left" />
    <Handle type="source" :position="Position.Right" />
  </div>
</template>

<script setup lang="ts">
import { Handle, Position } from "@vue-flow/core";

defineProps<{
  data: {
    name: string;
    status: string;
    terminal: boolean;
    isStart: boolean;
  };
}>();
</script>
