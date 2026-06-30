import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { mount } from "@vue/test-utils";
import TrashView from "./TrashView.vue";

const mockInvoke = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

const sampleItems = [
  {
    harness_id: "h1",
    name: "My Harness",
    deleted_at: "2026-05-01T12:00:00Z",
    remaining_days: 60,
  },
  {
    harness_id: "h2",
    name: "Old Harness",
    deleted_at: "2026-06-10T08:00:00Z",
    remaining_days: 20,
  },
];

/** flush all pending micro-tasks and timers once */
async function flush() {
  await new Promise((r) => setTimeout(r, 0));
}

beforeEach(() => {
  mockInvoke.mockResolvedValue(sampleItems);
});

afterEach(() => {
  vi.clearAllMocks();
  vi.useRealTimers();
});

describe("TrashView", () => {
  it("calls list_trash on mount", async () => {
    mount(TrashView);
    await flush();
    expect(mockInvoke).toHaveBeenCalledWith("list_trash");
  });

  it("renders harness names", async () => {
    const wrapper = mount(TrashView);
    await flush();
    expect(wrapper.text()).toContain("My Harness");
    expect(wrapper.text()).toContain("Old Harness");
  });

  it("shows deleted_at date formatted as YYYY-MM-DD", async () => {
    const wrapper = mount(TrashView);
    await flush();
    expect(wrapper.text()).toContain("2026-05-01");
    expect(wrapper.text()).toContain("2026-06-10");
  });

  it("shows remaining_days", async () => {
    const wrapper = mount(TrashView);
    await flush();
    expect(wrapper.text()).toContain("60");
    expect(wrapper.text()).toContain("20");
  });

  it("applies text-red-500 class when remaining_days <= 30", async () => {
    const wrapper = mount(TrashView);
    await flush();
    const spans = wrapper.findAll("span");
    const redSpan = spans.find((s) => s.classes().includes("text-red-500"));
    expect(redSpan).toBeDefined();
    expect(redSpan!.text()).toContain("20");
  });

  it("does NOT apply text-red-500 when remaining_days > 30", async () => {
    const wrapper = mount(TrashView);
    await flush();
    const spans = wrapper.findAll("span");
    const normalSpan = spans.find(
      (s) => s.text().includes("60") && !s.classes().includes("text-red-500"),
    );
    expect(normalSpan).toBeDefined();
  });

  it("shows empty state message when list is empty", async () => {
    mockInvoke.mockResolvedValue([]);
    const wrapper = mount(TrashView);
    await flush();
    expect(wrapper.text()).toContain("ゴミ箱は空です");
  });

  it("renders 復活 and 完全削除 buttons for each item", async () => {
    const wrapper = mount(TrashView);
    await flush();
    const restoreBtns = wrapper.findAll("[data-testid='restore-btn']");
    const purgeBtns = wrapper.findAll("[data-testid='purge-btn']");
    expect(restoreBtns).toHaveLength(2);
    expect(purgeBtns).toHaveLength(2);
  });

  it("calls restore_harness with correct harnessId on 復活 click", async () => {
    const wrapper = mount(TrashView);
    await flush();
    mockInvoke.mockResolvedValue([]);
    await wrapper.findAll("[data-testid='restore-btn']")[0].trigger("click");
    await flush();
    expect(mockInvoke).toHaveBeenCalledWith("restore_harness", { harnessId: "h1" });
  });

  it("refreshes list after 復活", async () => {
    const wrapper = mount(TrashView);
    await flush();
    mockInvoke.mockResolvedValue([sampleItems[1]]);
    await wrapper.findAll("[data-testid='restore-btn']")[0].trigger("click");
    await flush();
    // list_trash called again after restore
    const listCalls = mockInvoke.mock.calls.filter((c) => c[0] === "list_trash");
    expect(listCalls.length).toBeGreaterThanOrEqual(2);
  });

  it("shows confirmation dialog when 完全削除 is clicked", async () => {
    const wrapper = mount(TrashView);
    await flush();
    expect(wrapper.find("[data-testid='purge-dialog']").exists()).toBe(false);
    await wrapper.findAll("[data-testid='purge-btn']")[0].trigger("click");
    expect(wrapper.find("[data-testid='purge-dialog']").exists()).toBe(true);
  });

  it("dialog contains required warning text", async () => {
    const wrapper = mount(TrashView);
    await flush();
    await wrapper.findAll("[data-testid='purge-btn']")[0].trigger("click");
    const dialog = wrapper.find("[data-testid='purge-dialog']");
    expect(dialog.text()).toContain("完全に削除されます");
    expect(dialog.text()).toContain("元に戻せません");
  });

  it("hides dialog on cancel", async () => {
    const wrapper = mount(TrashView);
    await flush();
    await wrapper.findAll("[data-testid='purge-btn']")[0].trigger("click");
    await wrapper.find("[data-testid='purge-cancel-btn']").trigger("click");
    expect(wrapper.find("[data-testid='purge-dialog']").exists()).toBe(false);
  });

  it("calls purge_harness with correct harnessId on confirm", async () => {
    const wrapper = mount(TrashView);
    await flush();
    await wrapper.findAll("[data-testid='purge-btn']")[1].trigger("click");
    mockInvoke.mockResolvedValue([]);
    await wrapper.find("[data-testid='purge-confirm-btn']").trigger("click");
    await flush();
    expect(mockInvoke).toHaveBeenCalledWith("purge_harness", { harnessId: "h2" });
  });

  it("refreshes list after purge", async () => {
    const wrapper = mount(TrashView);
    await flush();
    await wrapper.findAll("[data-testid='purge-btn']")[0].trigger("click");
    mockInvoke.mockResolvedValue([]);
    await wrapper.find("[data-testid='purge-confirm-btn']").trigger("click");
    await flush();
    const listCalls = mockInvoke.mock.calls.filter((c) => c[0] === "list_trash");
    expect(listCalls.length).toBeGreaterThanOrEqual(2);
  });

  it("polls list_trash every 2000ms", async () => {
    vi.useFakeTimers();
    mount(TrashView);
    await vi.advanceTimersByTimeAsync(0);
    const callsBefore = mockInvoke.mock.calls.filter((c) => c[0] === "list_trash").length;
    await vi.advanceTimersByTimeAsync(2000);
    const callsAfter = mockInvoke.mock.calls.filter((c) => c[0] === "list_trash").length;
    expect(callsAfter).toBeGreaterThan(callsBefore);
  });

  it("clears poll interval on unmount", async () => {
    vi.useFakeTimers();
    const clearSpy = vi.spyOn(globalThis, "clearInterval");
    const wrapper = mount(TrashView);
    await vi.advanceTimersByTimeAsync(0);
    wrapper.unmount();
    expect(clearSpy).toHaveBeenCalled();
  });
});
