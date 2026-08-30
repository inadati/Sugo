import { describe, it, expect } from "vitest";
import { mount } from "@vue/test-utils";
import HarnessContextMenu from "./HarnessContextMenu.vue";

const folders = [
  { folder_id: "f1", name: "開発", harness_count: 1 },
  { folder_id: "f2", name: "調査", harness_count: 0 },
];

describe("HarnessContextMenu", () => {
  it("フォルダ一覧と未分類を提示する", () => {
    const wrapper = mount(HarnessContextMenu, {
      props: { x: 0, y: 0, harnessId: "h1", currentFolderId: null, folders },
    });
    expect(wrapper.text()).toContain("開発");
    expect(wrapper.text()).toContain("調査");
    expect(wrapper.text()).toContain("未分類");
  });

  it("フォルダを選ぶと move イベントを発火する", async () => {
    const wrapper = mount(HarnessContextMenu, {
      props: { x: 0, y: 0, harnessId: "h1", currentFolderId: null, folders },
    });
    await wrapper.get('[data-testid="move-to-f1"]').trigger("click");
    expect(wrapper.emitted("move")![0]).toEqual([{ harnessId: "h1", folderId: "f1" }]);
  });

  it("未分類を選ぶと folderId が null の move を発火する", async () => {
    const wrapper = mount(HarnessContextMenu, {
      props: { x: 0, y: 0, harnessId: "h1", currentFolderId: "f1", folders },
    });
    await wrapper.get('[data-testid="move-to-uncategorized"]').trigger("click");
    expect(wrapper.emitted("move")![0]).toEqual([{ harnessId: "h1", folderId: null }]);
  });

  it("「名前を変更」クリックで rename と close を emit する", async () => {
    const wrapper = mount(HarnessContextMenu, {
      props: { x: 0, y: 0, harnessId: "h1", currentFolderId: null, folders: [] },
    });

    await wrapper.find('[data-testid="rename-from-menu"]').trigger("click");

    expect(wrapper.emitted("rename")).toEqual([["h1"]]);
    expect(wrapper.emitted("close")).toBeTruthy();
  });

  it("ゴミ箱へ移動を選ぶと trash イベントを発火する", async () => {
    const wrapper = mount(HarnessContextMenu, {
      props: { x: 0, y: 0, harnessId: "h1", currentFolderId: null, folders },
    });
    await wrapper.get('[data-testid="trash-from-menu"]').trigger("click");
    expect(wrapper.emitted("trash")![0]).toEqual(["h1"]);
  });

  it("絵文字を使わない", () => {
    const wrapper = mount(HarnessContextMenu, {
      props: { x: 0, y: 0, harnessId: "h1", currentFolderId: null, folders },
    });
    expect(wrapper.text()).not.toMatch(/\p{Extended_Pictographic}/u);
  });
});
