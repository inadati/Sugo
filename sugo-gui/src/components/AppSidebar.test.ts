import { describe, it, expect, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { createRouter, createMemoryHistory } from "vue-router";
import { invoke } from "@tauri-apps/api/core";
import AppSidebar from "./AppSidebar.vue";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn((cmd: string) => {
    if (cmd === "list_folders") {
      return Promise.resolve([
        { folder_id: "f1", name: "開発", harness_count: 3 },
        { folder_id: "f2", name: "調査", harness_count: 0 },
      ]);
    }
    if (cmd === "list_trash") return Promise.resolve([]);
    return Promise.resolve([]);
  }),
}));

function makeRouter() {
  return createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/", component: { template: "<div/>" } },
      { path: "/uncategorized", component: { template: "<div/>" } },
      { path: "/folder/:id", component: { template: "<div/>" } },
      { path: "/trash", component: { template: "<div/>" } },
    ],
  });
}

describe("AppSidebar", () => {
  it("フォルダ名と件数を表示する", async () => {
    const router = makeRouter();
    const wrapper = mount(AppSidebar, { global: { plugins: [router] } });
    await new Promise((r) => setTimeout(r, 0));
    expect(wrapper.text()).toContain("開発");
    expect(wrapper.text()).toContain("3");
    expect(wrapper.text()).toContain("調査");
  });

  it("未分類を常に表示する", async () => {
    const router = makeRouter();
    const wrapper = mount(AppSidebar, { global: { plugins: [router] } });
    await new Promise((r) => setTimeout(r, 0));
    expect(wrapper.text()).toContain("未分類");
  });

  it("絵文字を使わない", async () => {
    const router = makeRouter();
    const wrapper = mount(AppSidebar, { global: { plugins: [router] } });
    await new Promise((r) => setTimeout(r, 0));
    expect(wrapper.text()).not.toMatch(/\p{Extended_Pictographic}/u);
  });
});

describe("AppSidebar – drag & drop", () => {
  it("フォルダ行にドロップすると move_harness_to_folder を呼ぶ", async () => {
    const router = makeRouter();
    const wrapper = mount(AppSidebar, { global: { plugins: [router] } });
    await new Promise((r) => setTimeout(r, 0));
    const target = wrapper.get('[data-testid="folder-drop-f1"]');
    await target.trigger("dragover");
    await target.trigger("drop", {
      dataTransfer: { getData: () => "h1" },
    });
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("move_harness_to_folder", {
      harnessId: "h1",
      folderId: "f1",
    });
  });

  it("未分類行にドロップすると folderId が null になる", async () => {
    const router = makeRouter();
    const wrapper = mount(AppSidebar, { global: { plugins: [router] } });
    await new Promise((r) => setTimeout(r, 0));
    const target = wrapper.get('[data-testid="folder-drop-uncategorized"]');
    await target.trigger("dragover");
    await target.trigger("drop", { dataTransfer: { getData: () => "h1" } });
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("move_harness_to_folder", {
      harnessId: "h1",
      folderId: null,
    });
  });
});
