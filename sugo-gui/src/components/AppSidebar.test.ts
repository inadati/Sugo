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

  it("ドロップが失敗（存在しないフォルダ）した場合トーストを表示し一覧を再取得する", async () => {
    const router = makeRouter();
    const wrapper = mount(AppSidebar, { global: { plugins: [router] } });
    await new Promise((r) => setTimeout(r, 0));
    const callsBefore = vi.mocked(invoke).mock.calls.filter((c) => c[0] === "list_folders").length;
    vi.mocked(invoke).mockImplementationOnce(() => Promise.reject(new Error("not found: f1")));
    const target = wrapper.get('[data-testid="folder-drop-f1"]');
    await target.trigger("dragover");
    await target.trigger("drop", { dataTransfer: { getData: () => "h1" } });
    await new Promise((r) => setTimeout(r, 0));
    expect(wrapper.text()).toMatch(/見つかりません|移動に失敗/);
    const callsAfter = vi.mocked(invoke).mock.calls.filter((c) => c[0] === "list_folders").length;
    expect(callsAfter).toBeGreaterThan(callsBefore);
  });

  it("ドロップが DB 障害で失敗した場合はエラー表示のみで一覧を再取得しない", async () => {
    const router = makeRouter();
    const wrapper = mount(AppSidebar, { global: { plugins: [router] } });
    await new Promise((r) => setTimeout(r, 0));
    const callsBefore = vi.mocked(invoke).mock.calls.filter((c) => c[0] === "list_folders").length;
    vi.mocked(invoke).mockImplementationOnce(() => Promise.reject(new Error("storage error: disk full")));
    const target = wrapper.get('[data-testid="folder-drop-f1"]');
    await target.trigger("dragover");
    await target.trigger("drop", { dataTransfer: { getData: () => "h1" } });
    await new Promise((r) => setTimeout(r, 0));
    expect(wrapper.text()).toContain("失敗しました");
    const callsAfter = vi.mocked(invoke).mock.calls.filter((c) => c[0] === "list_folders").length;
    expect(callsAfter).toBe(callsBefore);
  });
});

describe("AppSidebar – フォルダ削除の異常系と遷移", () => {
  it("削除に失敗した場合トーストを表示し一覧を再取得する", async () => {
    const router = makeRouter();
    const wrapper = mount(AppSidebar, { global: { plugins: [router] } });
    await new Promise((r) => setTimeout(r, 0));
    const callsBefore = vi.mocked(invoke).mock.calls.filter((c) => c[0] === "list_folders").length;
    vi.mocked(invoke).mockImplementationOnce(() => Promise.reject(new Error("not found: f1")));
    const rows = wrapper.findAll('[data-testid="delete-folder-btn"]');
    await rows[0].trigger("click");
    await new Promise((r) => setTimeout(r, 0));
    expect(wrapper.text()).toMatch(/見つかりません|削除に失敗/);
    const callsAfter = vi.mocked(invoke).mock.calls.filter((c) => c[0] === "list_folders").length;
    expect(callsAfter).toBeGreaterThan(callsBefore);
  });

  it("削除中のフォルダを表示していた場合はすべてのハーネスへ遷移する", async () => {
    const router = makeRouter();
    await router.push("/folder/f1");
    await router.isReady();
    const wrapper = mount(AppSidebar, { global: { plugins: [router] } });
    await new Promise((r) => setTimeout(r, 0));
    const rows = wrapper.findAll('[data-testid="delete-folder-btn"]');
    await rows[0].trigger("click");
    await new Promise((r) => setTimeout(r, 0));
    expect(router.currentRoute.value.path).toBe("/");
  });

  it("表示していないフォルダを削除しても遷移しない", async () => {
    const router = makeRouter();
    await router.push("/folder/f2");
    await router.isReady();
    const wrapper = mount(AppSidebar, { global: { plugins: [router] } });
    await new Promise((r) => setTimeout(r, 0));
    const rows = wrapper.findAll('[data-testid="delete-folder-btn"]');
    await rows[0].trigger("click"); // f1 を削除する（現在表示中は f2）
    await new Promise((r) => setTimeout(r, 0));
    expect(router.currentRoute.value.path).toBe("/folder/f2");
  });
});

describe("AppSidebar – フォルダ改名の異常系", () => {
  it("改名対象が削除済み(NotFound)の場合はダイアログを閉じてトースト表示し一覧を再取得する", async () => {
    const router = makeRouter();
    const wrapper = mount(AppSidebar, { global: { plugins: [router] } });
    await new Promise((r) => setTimeout(r, 0));
    const callsBefore = vi.mocked(invoke).mock.calls.filter((c) => c[0] === "list_folders").length;

    await wrapper.get('[data-testid="rename-folder-btn"]').trigger("click");
    vi.mocked(invoke).mockImplementationOnce(() => Promise.reject(new Error("not found: f1")));
    await wrapper.get('[data-testid="folder-name"]').setValue("開発2");
    await wrapper.get('[data-testid="folder-submit"]').trigger("click");
    await new Promise((r) => setTimeout(r, 0));

    // ダイアログが閉じている（エラー文字列をダイアログ内には表示しない）
    expect(wrapper.find('[data-testid="folder-name"]').exists()).toBe(false);
    expect(wrapper.text()).toContain("見つかりません");
    const callsAfter = vi.mocked(invoke).mock.calls.filter((c) => c[0] === "list_folders").length;
    expect(callsAfter).toBeGreaterThan(callsBefore);
  });

  it("バリデーション/重複エラーの場合はダイアログを開いたまま再入力できる", async () => {
    const router = makeRouter();
    const wrapper = mount(AppSidebar, { global: { plugins: [router] } });
    await new Promise((r) => setTimeout(r, 0));

    await wrapper.get('[data-testid="rename-folder-btn"]').trigger("click");
    vi.mocked(invoke).mockImplementationOnce(() =>
      Promise.reject(new Error("conflict: フォルダ「調査」は既に存在します"))
    );
    await wrapper.get('[data-testid="folder-name"]').setValue("調査");
    await wrapper.get('[data-testid="folder-submit"]').trigger("click");
    await new Promise((r) => setTimeout(r, 0));

    // ダイアログは閉じずに残り、エラーはダイアログ内に表示される
    expect(wrapper.find('[data-testid="folder-name"]').exists()).toBe(true);
    expect(wrapper.text()).toContain("既に存在");
  });
});
