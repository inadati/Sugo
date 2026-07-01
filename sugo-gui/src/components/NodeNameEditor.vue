<template>
  <input
    ref="inputEl"
    v-model="name"
    type="text"
    class="absolute z-20 border-2 border-blue-400 rounded px-1 py-0.5 text-sm text-center shadow"
    :style="{ left: x + 'px', top: y + 'px', width: width + 'px', transform: 'translate(-50%, -50%)' }"
    @keydown.enter="commit"
    @keydown.esc="cancel"
    @blur="commit"
  />
</template>

<script setup lang="ts">
import { ref, onMounted } from "vue";

const props = defineProps<{ initialName: string; x: number; y: number; width: number }>();
const emit = defineEmits<{ commit: [name: string]; cancel: [] }>();

const name = ref(props.initialName);
const inputEl = ref<HTMLInputElement | null>(null);
// commit / cancel は一度だけ発火させる。Escape 取消後の blur 等による
// 二重発火（旧名での意図しない rename）を防ぐ。
let done = false;

onMounted(() => {
  inputEl.value?.focus();
  inputEl.value?.select();
});

function commit() {
  if (done) return;
  const trimmed = name.value.trim();
  if (!trimmed) {
    done = true;
    emit("cancel");
    return;
  }
  done = true;
  emit("commit", trimmed);
}

function cancel() {
  if (done) return;
  done = true;
  emit("cancel");
}
</script>
