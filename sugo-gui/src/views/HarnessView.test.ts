import { describe, it, expect, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { createRouter, createMemoryHistory } from "vue-router";
import HarnessView from "./HarnessView.vue";

const mockDetail = vi.hoisted(() => ({
  harness_id: "h1", name: "my-harness", current_version: 1,
  lock_version: 0, has_draft: true,
  cells: [
    { id: "c1", name: "start", status: "active", terminal: false },
    { id: "c2", name: "draft-one", status: "draft", terminal: true },
  ],
  edges: [],
  draft_diff: [{ cell_id: "c2", name: "draft-one" }],
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(mockDetail),
}));

vi.mock("../components/BoardGraph.vue", () => ({ default: { template: "<div/>" } }));
vi.mock("../components/AddCellDialog.vue", () => ({ default: { template: "<div/>" } }));

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
});
