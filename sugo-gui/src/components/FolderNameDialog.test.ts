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

  it("NotFound（対象フォルダが削除済み）の場合はダイアログ内表示ではなくnot-foundイベントを発火する", async () => {
    vi.mocked(invoke).mockRejectedValue(new Error("not found: f1"));
    const wrapper = mount(FolderNameDialog, {
      props: { mode: "rename", folderId: "f1", initialName: "開発" },
    });
    await wrapper.get('[data-testid="folder-name"]').setValue("開発2");
    await wrapper.get('[data-testid="folder-submit"]').trigger("click");
    await new Promise((r) => setTimeout(r, 0));
    expect(wrapper.emitted("not-found")).toBeTruthy();
    // NotFoundは呼び出し元がトースト+一覧再取得で扱うため、ダイアログ内には
    // 生のエラー文字列を表示しない。
    expect(wrapper.text()).not.toContain("not found");
  });

  it("Conflict（同名フォルダ）はNotFoundと違いダイアログ内に表示し、not-foundは発火しない", async () => {
    vi.mocked(invoke).mockRejectedValue(new Error("conflict: フォルダ「開発」は既に存在します"));
    const wrapper = mount(FolderNameDialog, {
      props: { mode: "rename", folderId: "f1", initialName: "旧" },
    });
    await wrapper.get('[data-testid="folder-name"]').setValue("開発");
    await wrapper.get('[data-testid="folder-submit"]').trigger("click");
    await new Promise((r) => setTimeout(r, 0));
    expect(wrapper.text()).toContain("既に存在");
    expect(wrapper.emitted("not-found")).toBeUndefined();
    expect(wrapper.emitted("close")).toBeUndefined();
  });

  it("entity=harness のとき rename_harness を invoke する", async () => {
    vi.mocked(invoke).mockResolvedValue({});
    const wrapper = mount(FolderNameDialog, {
      props: { mode: "rename", entity: "harness", harnessId: "h1", initialName: "alpha" },
    });
    await wrapper.get('[data-testid="folder-name"]').setValue("beta");
    await wrapper.get('[data-testid="folder-submit"]').trigger("click");
    await new Promise((r) => setTimeout(r, 0));

    expect(invoke).toHaveBeenCalledWith("rename_harness", { harnessId: "h1", name: "beta" });
    expect(wrapper.emitted("saved")).toBeTruthy();
  });

  it("entity=harness のとき見出しとラベルがハーネス向けになる", () => {
    const wrapper = mount(FolderNameDialog, {
      props: { mode: "rename", entity: "harness", harnessId: "h1", initialName: "alpha" },
    });
    expect(wrapper.text()).toContain("ハーネス名を変更");
    expect(wrapper.text()).not.toContain("フォルダ名を変更");
  });

  it("entity=harness で空名を弾くメッセージがハーネス向けになる", async () => {
    const wrapper = mount(FolderNameDialog, {
      props: { mode: "rename", entity: "harness", harnessId: "h1", initialName: "alpha" },
    });
    const callsBefore = vi.mocked(invoke).mock.calls.length;
    await wrapper.get('[data-testid="folder-name"]').setValue("   ");
    await wrapper.get('[data-testid="folder-submit"]').trigger("click");
    await new Promise((r) => setTimeout(r, 0));

    expect(wrapper.text()).toContain("ハーネス名を入力してください。");
    expect(vi.mocked(invoke).mock.calls.length).toBe(callsBefore);
  });
});
