import { describe, it, expect, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { createRouter, createMemoryHistory } from "vue-router";
import HarnessList from "./HarnessList.vue";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn((cmd: string) => {
    if (cmd === "list_harnesses") {
      return Promise.resolve([
        {
          harness_id: "h1",
          name: "alpha",
          current_version: 1,
          has_draft: false,
          folder_id: "f1",
          folder_name: "開発",
        },
        {
          harness_id: "h2",
          name: "beta",
          current_version: 2,
          has_draft: true,
          folder_id: null,
          folder_name: null,
        },
      ]);
    }
    if (cmd === "list_folders") {
      return Promise.resolve([{ folder_id: "f1", name: "開発", harness_count: 1 }]);
    }
    return Promise.resolve([]);
  }),
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

describe("HarnessList – create", () => {
  it("shows create button and opens NewHarnessDialog on click", async () => {
    const wrapper = mount(HarnessList, { global: { plugins: [makeRouter()] } });
    await new Promise((r) => setTimeout(r, 0));
    const btn = wrapper.find("[data-testid='create-harness-btn']");
    expect(btn.exists()).toBe(true);
    // 初期はダイアログ非表示
    expect(wrapper.find("[data-testid='name']").exists()).toBe(false);
    await btn.trigger("click");
    // NewHarnessDialog の名前入力が表示される
    expect(wrapper.find("[data-testid='name']").exists()).toBe(true);
  });
});

describe("HarnessList – スコープ切り替え", () => {
  it("フォルダスコープでは所属ハーネスだけを表示する", async () => {
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [
        { path: "/folder/:id", component: HarnessList },
        { path: "/harness/:id", component: { template: "<div/>" } },
      ],
    });
    router.push("/folder/f1");
    await router.isReady();
    const wrapper = mount(HarnessList, { global: { plugins: [router] } });
    await new Promise((r) => setTimeout(r, 0));
    expect(wrapper.text()).toContain("alpha");
    expect(wrapper.text()).not.toContain("beta");
  });

  it("未分類スコープでは folder_id が null のものだけを表示する", async () => {
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [
        { path: "/uncategorized", component: HarnessList },
        { path: "/harness/:id", component: { template: "<div/>" } },
      ],
    });
    router.push("/uncategorized");
    await router.isReady();
    const wrapper = mount(HarnessList, { global: { plugins: [router] } });
    await new Promise((r) => setTimeout(r, 0));
    expect(wrapper.text()).toContain("beta");
    expect(wrapper.text()).not.toContain("alpha");
  });

  it("フォルダスコープの見出しはフォルダ名になる", async () => {
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [
        { path: "/folder/:id", component: HarnessList },
        { path: "/harness/:id", component: { template: "<div/>" } },
      ],
    });
    router.push("/folder/f1");
    await router.isReady();
    const wrapper = mount(HarnessList, { global: { plugins: [router] } });
    await new Promise((r) => setTimeout(r, 0));
    expect(wrapper.find("h2").text()).toBe("開発");
  });

  it("未分類スコープの見出しは「未分類」になる", async () => {
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [
        { path: "/uncategorized", component: HarnessList },
        { path: "/harness/:id", component: { template: "<div/>" } },
      ],
    });
    router.push("/uncategorized");
    await router.isReady();
    const wrapper = mount(HarnessList, { global: { plugins: [router] } });
    await new Promise((r) => setTimeout(r, 0));
    expect(wrapper.find("h2").text()).toBe("未分類");
  });
});
