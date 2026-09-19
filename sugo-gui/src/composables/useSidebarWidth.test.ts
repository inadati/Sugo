import { describe, it, expect, beforeEach, vi, afterEach } from "vitest";
import {
  useSidebarWidth,
  clampSidebarWidth,
  SIDEBAR_MIN_WIDTH,
  SIDEBAR_MAX_WIDTH,
  SIDEBAR_DEFAULT_WIDTH,
} from "./useSidebarWidth";

const STORAGE_KEY = "sugo.sidebarWidth";

describe("clampSidebarWidth", () => {
  it("範囲内の値はそのまま返す", () => {
    expect(clampSidebarWidth(240)).toBe(240);
  });

  it("下限・上限でクランプする", () => {
    expect(clampSidebarWidth(SIDEBAR_MIN_WIDTH - 100)).toBe(SIDEBAR_MIN_WIDTH);
    expect(clampSidebarWidth(SIDEBAR_MAX_WIDTH + 100)).toBe(SIDEBAR_MAX_WIDTH);
  });

  it("小数は丸める（CSS の px にサブピクセルを渡さない）", () => {
    expect(clampSidebarWidth(240.6)).toBe(241);
  });
});

describe("useSidebarWidth", () => {
  beforeEach(() => {
    localStorage.clear();
    useSidebarWidth().resetWidth();
  });

  it("setWidth で幅が変わり localStorage に永続化される", () => {
    const { width, setWidth } = useSidebarWidth();
    setWidth(300);
    expect(width.value).toBe(300);
    expect(localStorage.getItem(STORAGE_KEY)).toBe("300");
  });

  it("永続化されるのはクランプ後の値", () => {
    const { setWidth } = useSidebarWidth();
    setWidth(9999);
    expect(localStorage.getItem(STORAGE_KEY)).toBe(String(SIDEBAR_MAX_WIDTH));
  });

  it("resetWidth で既定幅に戻る", () => {
    const { width, setWidth, resetWidth } = useSidebarWidth();
    setWidth(300);
    resetWidth();
    expect(width.value).toBe(SIDEBAR_DEFAULT_WIDTH);
  });

  it("複数の呼び出し元が同じ幅を共有する", () => {
    const a = useSidebarWidth();
    const b = useSidebarWidth();
    a.setWidth(260);
    expect(b.width.value).toBe(260);
  });

  it("localStorage が書けなくても幅の変更自体は成功する", () => {
    const spy = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("QuotaExceededError");
    });
    const { width, setWidth } = useSidebarWidth();
    expect(() => setWidth(300)).not.toThrow();
    expect(width.value).toBe(300);
    spy.mockRestore();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });
});
