import { describe, it, expect, vi } from "vitest";
import { mount } from "@vue/test-utils";
import AddCellDialog from "./AddCellDialog.vue";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue({ new_version: 2, lock_version: 1 }),
}));

describe("AddCellDialog", () => {
  it("emits close when cancel clicked", async () => {
    const wrapper = mount(AddCellDialog, {
      props: { harnessId: "h1", lockVersion: 0 },
    });
    await wrapper.find('[data-testid="cancel"]').trigger("click");
    expect(wrapper.emitted("close")).toBeTruthy();
  });

  it("does not submit with empty name", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    const wrapper = mount(AddCellDialog, {
      props: { harnessId: "h1", lockVersion: 0 },
    });
    await wrapper.find('[data-testid="submit"]').trigger("click");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("calls add_cell and emits added on success", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    const wrapper = mount(AddCellDialog, {
      props: { harnessId: "h1", lockVersion: 0 },
    });
    await wrapper.find('input').setValue("new-cell");
    await wrapper.find('[data-testid="submit"]').trigger("click");
    await new Promise(r => setTimeout(r, 0));
    expect(invoke).toHaveBeenCalledWith("add_cell", {
      harnessId: "h1", cellName: "new-cell", lockVersion: 0,
    });
    expect(wrapper.emitted("added")).toBeTruthy();
  });

  it("shows lock conflict message when invoke rejects with lock_conflict", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    // backend(commands.rs map_core_error)が実際に返す安定コードと同一文字列で検証する
    vi.mocked(invoke).mockRejectedValueOnce("lock_conflict");
    const wrapper = mount(AddCellDialog, {
      props: { harnessId: "h1", lockVersion: 0 },
    });
    await wrapper.find("input").setValue("new-cell");
    await wrapper.find('[data-testid="submit"]').trigger("click");
    await new Promise(r => setTimeout(r, 0));
    expect(wrapper.text()).toContain("他で編集が入りました。再読み込みしてください。");
    expect(wrapper.emitted("close")).toBeFalsy();
  });

  it("shows generic error message on non-lock-conflict error", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockRejectedValueOnce("database connection error");
    const wrapper = mount(AddCellDialog, {
      props: { harnessId: "h1", lockVersion: 0 },
    });
    await wrapper.find("input").setValue("new-cell");
    await wrapper.find('[data-testid="submit"]').trigger("click");
    await new Promise(r => setTimeout(r, 0));
    expect(wrapper.text()).toContain("エラーが発生しました。");
    expect(wrapper.emitted("close")).toBeFalsy();
  });
});
