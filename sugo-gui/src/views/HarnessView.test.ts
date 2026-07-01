import { describe, it, expect, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { createRouter, createMemoryHistory } from "vue-router";
import HarnessView from "./HarnessView.vue";

const mockDetail = vi.hoisted(() => ({
  harness_id: "h1", name: "my-harness", current_version: 1,
  lock_version: 0, has_draft: true, start_cell_id: "c1",
  cells: [
    { id: "c1", name: "start", prompt: "do the thing", status: "active", terminal: false },
    { id: "c2", name: "draft-one", prompt: "", status: "draft", terminal: true },
  ],
  edges: [],
  draft_diff: [{ cell_id: "c2", name: "draft-one" }],
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(mockDetail),
}));

vi.mock("../components/BoardGraph.vue", () => ({
  default: { name: "BoardGraph", emits: ["select"], template: "<div/>" },
}));
vi.mock("../components/AddCellDialog.vue", () => ({ default: { name: "AddCellDialog", template: "<div/>" } }));
vi.mock("../components/CellDetailPanel.vue", () => ({
  default: { name: "CellDetailPanel", props: ["harnessId", "cell", "lockVersion"], template: "<div class='panel'/>" },
}));

describe("HarnessView", () => {
  const makeRouter = () => createRouter({
    history: createMemoryHistory(),
    routes: [{ path: "/harness/:id", component: HarnessView, props: true }],
  });

  it("shows harness name", async () => {
    const router = makeRouter();
    await router.push("/harness/h1");
    const wrapper = mount(HarnessView, { props: { id: "h1" }, global: { plugins: [router] } });
    await new Promise(r => setTimeout(r, 0));
    expect(wrapper.text()).toContain("my-harness");
  });

  it("shows draft_diff entries", async () => {
    const router = makeRouter();
    const wrapper = mount(HarnessView, { props: { id: "h1" }, global: { plugins: [router] } });
    await new Promise(r => setTimeout(r, 0));
    expect(wrapper.text()).toContain("draft-one");
  });

  it("opens CellDetailPanel when a cell is selected", async () => {
    const router = makeRouter();
    const wrapper = mount(HarnessView, { props: { id: "h1" }, global: { plugins: [router] } });
    await new Promise(r => setTimeout(r, 0));
    wrapper.findComponent({ name: "BoardGraph" }).vm.$emit("select", "c1");
    await wrapper.vm.$nextTick();
    expect(wrapper.findComponent({ name: "CellDetailPanel" }).exists()).toBe(true);
  });

  it("reloads detail when polled current_version changes", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.useFakeTimers();
    vi.mocked(invoke)
      .mockResolvedValueOnce({ ...mockDetail, current_version: 1 })
      .mockResolvedValue({ ...mockDetail, current_version: 2, lock_version: 1 });

    const router = makeRouter();
    const wrapper = mount(HarnessView, { props: { id: "h1" }, global: { plugins: [router] } });
    await vi.advanceTimersByTimeAsync(0);
    await vi.advanceTimersByTimeAsync(2000);
    const vm = wrapper.vm as unknown as { detail: { current_version: number } | null };
    expect(vm.detail?.current_version).toBe(2);
    vi.useRealTimers();
    vi.mocked(invoke).mockResolvedValue(mockDetail);
  });
});
