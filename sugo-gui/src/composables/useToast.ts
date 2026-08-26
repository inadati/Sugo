import { ref } from "vue";

/**
 * 画面下部/上部に一時表示するトーストの状態を管理する共有コンポーザブル。
 * HarnessView.vue で先行実装されていたローカルなトースト表示パターン
 * （3秒で自動的に消える）を、複数コンポーネントから使えるよう切り出したもの。
 */
export function useToast() {
  const toast = ref("");
  let timer: ReturnType<typeof setTimeout> | null = null;

  function showToast(msg: string) {
    toast.value = msg;
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => {
      if (toast.value === msg) toast.value = "";
    }, 3000);
  }

  return { toast, showToast };
}
