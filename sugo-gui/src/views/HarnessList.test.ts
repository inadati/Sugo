import { describe, it, expect, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { createRouter, createMemoryHistory } from "vue-router";
import HarnessList from "./HarnessList.vue";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue([
    { harness_id: "h1", name: "alpha", current_version: 1, has_draft: false },
    { harness_id: "h2", name: "beta", current_version: 2, has_draft: true },
  ]),
}));

describe("HarnessList", () => {
  it("shows harness names", async () => {
    const router = createRouter({ history: createMemoryHistory(), routes: [
      { path: "/", component: HarnessList },
      { path: "/harness/:id", component: { template: "<div/>" } },
    ]});
    const wrapper = mount(HarnessList, { global: { plugins: [router] } });
    await new Promise(r => setTimeout(r, 0)); // flush async
    expect(wrapper.text()).toContain("alpha");
    expect(wrapper.text()).toContain("beta");
  });

  it("shows draft badge for has_draft harnesses", async () => {
    const router = createRouter({ history: createMemoryHistory(), routes: [
      { path: "/", component: HarnessList },
      { path: "/harness/:id", component: { template: "<div/>" } },
    ]});
    const wrapper = mount(HarnessList, { global: { plugins: [router] } });
    await new Promise(r => setTimeout(r, 0));
    expect(wrapper.text()).toContain("DRAFT");
  });
});

function makeRouter() {
  return createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/", component: HarnessList },
      { path: "/harness/:id", component: { template: "<div/>" } },
    ],
  });
}

describe("HarnessList – trash confirmation", () => {
  it("shows confirmation dialog when trash icon clicked", async () => {
    const wrapper = mount(HarnessList, { global: { plugins: [makeRouter()] } });
    await new Promise((r) => setTimeout(r, 0));
    // ダイアログは初期非表示
    expect(wrapper.find("[data-testid='trash-dialog']").exists()).toBe(false);
    // ゴミ箱ボタンをクリック（h1 の行）
    await wrapper.findAll("[data-testid='trash-btn']")[0].trigger("click");
    expect(wrapper.find("[data-testid='trash-dialog']").exists()).toBe(true);
    expect(wrapper.text()).toContain("alpha");
  });

  it("hides dialog on cancel", async () => {
    const wrapper = mount(HarnessList, { global: { plugins: [makeRouter()] } });
    await new Promise((r) => setTimeout(r, 0));
    await wrapper.findAll("[data-testid='trash-btn']")[0].trigger("click");
    await wrapper.find("[data-testid='trash-cancel-btn']").trigger("click");
    expect(wrapper.find("[data-testid='trash-dialog']").exists()).toBe(false);
  });
});
