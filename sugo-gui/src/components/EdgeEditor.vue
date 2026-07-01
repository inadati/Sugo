<template>
  <div class="fixed inset-0 z-50" @click.self="$emit('close')">
    <div
      class="absolute bg-white rounded-lg shadow-xl border border-gray-200 p-4 w-72"
      :style="anchorStyle"
    >
      <p class="text-sm text-gray-500 mb-3">
        <span class="font-medium text-gray-700">{{ fromName }}</span>
        <span class="mx-1">→</span>
        <span class="font-medium text-gray-700">{{ toName }}</span>
      </p>

      <label class="block text-xs text-gray-500 mb-1">ラベル（必須）</label>
      <input
        data-testid="edge-label"
        v-model="label"
        type="text"
        placeholder="例: 次へ"
        class="w-full border border-gray-300 rounded px-2 py-1 mb-2 focus:outline-none focus:border-blue-400"
        @keydown.enter="submit"
      />
      <label class="block text-xs text-gray-500 mb-1">ガード条件（任意）</label>
      <input
        data-testid="edge-guard"
        v-model="guard"
        type="text"
        placeholder="例: 続ける"
        class="w-full border border-gray-300 rounded px-2 py-1 mb-3 focus:outline-none focus:border-blue-400"
        @keydown.enter="submit"
      />
      <p v-if="errorMsg" class="text-red-500 text-xs mb-2">{{ errorMsg }}</p>
      <div class="flex justify-end gap-2">
        <button data-testid="edge-cancel" class="px-3 py-1 text-gray-600 hover:bg-gray-100 rounded text-sm" @click="$emit('close')">キャンセル</button>
        <button data-testid="edge-submit" class="px-3 py-1 bg-blue-500 text-white rounded hover:bg-blue-600 disabled:opacity-50 text-sm" :disabled="submitting" @click="submit">保存</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";

const props = defineProps<{
  harnessId: string;
  mode: "add" | "edit";
  from: string;
  to: string;
  fromName: string;
  toName: string;
  oldLabel?: string;
  initialLabel?: string;
  initialGuard?: string | null;
  lockVersion: number;
  anchorX?: number;
  anchorY?: number;
}>();
const emit = defineEmits<{
  close: [];
  saved: [newVersion: number, lockVersion: number];
  reload: [message: string];
}>();

const label = ref(props.initialLabel ?? "");
const guard = ref(props.initialGuard ?? "");
const errorMsg = ref("");
const submitting = ref(false);

// ポップオーバー（w-72 = 288px、想定高さ ~220px）がビューポート端で
// 切れないよう、アンカー座標を画面内にクランプする。
const POPOVER_W = 288;
const POPOVER_H = 220;
const MARGIN = 8;
const anchorStyle = computed(() => {
  const rawX = props.anchorX ?? window.innerWidth / 2 - POPOVER_W / 2;
  const rawY = props.anchorY ?? window.innerHeight / 2 - POPOVER_H / 2;
  const maxX = Math.max(MARGIN, window.innerWidth - POPOVER_W - MARGIN);
  const maxY = Math.max(MARGIN, window.innerHeight - POPOVER_H - MARGIN);
  return {
    left: Math.min(Math.max(rawX, MARGIN), maxX) + "px",
    top: Math.min(Math.max(rawY, MARGIN), maxY) + "px",
  };
});

async function submit() {
  if (submitting.value) return; // 送信中の二重送信（Enter連打）を防ぐ
  if (!label.value.trim()) {
    errorMsg.value = "ラベルを入力してください。";
    return;
  }
  submitting.value = true;
  errorMsg.value = "";
  try {
    let result: { new_version: number; lock_version: number };
    if (props.mode === "add") {
      result = await invoke("add_edge", {
        harnessId: props.harnessId, from: props.from, to: props.to,
        label: label.value.trim(), guard: guard.value.trim() || null, lockVersion: props.lockVersion,
      });
    } else {
      result = await invoke("update_edge", {
        harnessId: props.harnessId, from: props.from, to: props.to, oldLabel: props.oldLabel,
        newLabel: label.value.trim(), newGuard: guard.value.trim() || null, lockVersion: props.lockVersion,
      });
    }
    emit("saved", result.new_version, result.lock_version);
  } catch (e) {
    const msg = String(e);
    // 盤面がずれている系（並行編集）はポップオーバーを閉じ、親に再読込を促す（設計: トースト＋再読込による自己修復）
    if (msg.includes("lock_conflict")) {
      emit("reload", "他で編集が入りました。再読み込みします。");
      return;
    } else if (msg.includes("edge_not_found")) {
      emit("reload", "対象のエッジが見つかりません。再読み込みします。");
      return;
    } else if (msg.includes("duplicate_edge")) {
      errorMsg.value = "同じ経路・ラベルのエッジが既に存在します。";
    } else if (msg.includes("empty_label")) {
      errorMsg.value = "ラベルを入力してください。";
    } else {
      errorMsg.value = "エラーが発生しました。";
    }
  } finally {
    submitting.value = false;
  }
}
</script>
