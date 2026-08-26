import { describe, it, expect, vi } from "vitest";
import { mount } from "@vue/test-utils";
import FolderNameDialog from "./FolderNameDialog.vue";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("FolderNameDialog", () => {
  it("Enter キーでは保存しない（IME 変換確定と衝突するため）", async () => {
    vi.mocked(invoke).mockResolvedValue({ folder_id: "f1", name: "開発" });
    const wrapper = mount(FolderNameDialog, { props: { mode: "create" } });
    await wrapper.get('[data-testid="folder-name"]').setValue("開発");
    await wrapper.get('[data-testid="folder-name"]').trigger("keydown.enter");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("保存ボタンで create_folder を呼ぶ", async () => {
    vi.mocked(invoke).mockResolvedValue({ folder_id: "f1", name: "開発" });
    const wrapper = mount(FolderNameDialog, { props: { mode: "create" } });
    await wrapper.get('[data-testid="folder-name"]').setValue("開発");
    await wrapper.get('[data-testid="folder-submit"]').trigger("click");
    await new Promise((r) => setTimeout(r, 0));
    expect(invoke).toHaveBeenCalledWith("create_folder", { name: "開発" });
  });

  it("重複エラーを表示して閉じない", async () => {
    vi.mocked(invoke).mockRejectedValue("フォルダ「開発」は既に存在します");
    const wrapper = mount(FolderNameDialog, { props: { mode: "create" } });
    await wrapper.get('[data-testid="folder-name"]').setValue("開発");
    await wrapper.get('[data-testid="folder-submit"]').trigger("click");
    await new Promise((r) => setTimeout(r, 0));
    expect(wrapper.text()).toContain("既に存在");
    expect(wrapper.emitted("close")).toBeUndefined();
  });

  it("改名モードでは rename_folder を呼ぶ", async () => {
    vi.mocked(invoke).mockResolvedValue({});
    const wrapper = mount(FolderNameDialog, {
      props: { mode: "rename", folderId: "f1", initialName: "開発" },
    });
    await wrapper.get('[data-testid="folder-name"]').setValue("開発2");
    await wrapper.get('[data-testid="folder-submit"]').trigger("click");
    await new Promise((r) => setTimeout(r, 0));
    expect(invoke).toHaveBeenCalledWith("rename_folder", {
      folderId: "f1",
      name: "開発2",
    });
  });

  it("保存に成功すると saved イベントを発火する", async () => {
    vi.mocked(invoke).mockResolvedValue({ folder_id: "f1", name: "開発" });
    const wrapper = mount(FolderNameDialog, { props: { mode: "create" } });
    await wrapper.get('[data-testid="folder-name"]').setValue("開発");
    await wrapper.get('[data-testid="folder-submit"]').trigger("click");
    await new Promise((r) => setTimeout(r, 0));
    expect(wrapper.emitted("saved")).toBeTruthy();
  });

  it("キャンセルボタンで close イベントを発火する", async () => {
    const wrapper = mount(FolderNameDialog, { props: { mode: "create" } });
    await wrapper.get('[data-testid="folder-cancel"]').trigger("click");
    expect(wrapper.emitted("close")).toBeTruthy();
  });
});
