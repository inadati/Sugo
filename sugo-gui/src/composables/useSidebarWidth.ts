import { ref } from "vue";

export const SIDEBAR_MIN_WIDTH = 160;
export const SIDEBAR_MAX_WIDTH = 480;
export const SIDEBAR_DEFAULT_WIDTH = 200;

const STORAGE_KEY = "sugo.sidebarWidth";

export function clampSidebarWidth(px: number): number {
  return Math.min(SIDEBAR_MAX_WIDTH, Math.max(SIDEBAR_MIN_WIDTH, Math.round(px)));
}

function loadWidth(): number {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw === null) return SIDEBAR_DEFAULT_WIDTH;
    const parsed = Number(raw);
    if (!Number.isFinite(parsed)) return SIDEBAR_DEFAULT_WIDTH;
    return clampSidebarWidth(parsed);
  } catch {
    // localStorage が使えない環境（プライベートモード等）では既定値で動かす
    return SIDEBAR_DEFAULT_WIDTH;
  }
}

// モジュールスコープで保持し、複数コンポーネントから同じ幅を参照できるようにする。
const width = ref(loadWidth());

function persist(px: number) {
  try {
    localStorage.setItem(STORAGE_KEY, String(px));
  } catch {
    // 永続化に失敗してもセッション中の幅は維持されるため無視する
  }
}

export function useSidebarWidth() {
  function setWidth(px: number) {
    width.value = clampSidebarWidth(px);
    persist(width.value);
  }

  function resetWidth() {
    setWidth(SIDEBAR_DEFAULT_WIDTH);
  }

  return { width, setWidth, resetWidth };
}
