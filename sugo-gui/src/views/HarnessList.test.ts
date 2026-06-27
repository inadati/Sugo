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
