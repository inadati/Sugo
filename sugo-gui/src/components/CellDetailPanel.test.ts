import { describe, it, expect, vi } from "vitest";
import { mount } from "@vue/test-utils";
import CellDetailPanel from "./CellDetailPanel.vue";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue({ new_version: 2, lock_version: 1 }),
}));

const cell = { id: "c1", name: "old", prompt: "do the thing", status: "active", terminal: false };

describe("CellDetailPanel", () => {
  it("shows the cell prompt", () => {
    const wrapper = mount(CellDetailPanel, {
      props: { harnessId: "h1", cell, lockVersion: 0 },
    });
    expect(wrapper.text()).toContain("do the thing");
  });

  it("emits close when close button clicked", async () => {
    const wrapper = mount(CellDetailPanel, {
      props: { harnessId: "h1", cell, lockVersion: 0 },
    });
    await wrapper.find('[data-testid="panel-close"]').trigger("click");
    expect(wrapper.emitted("close")).toBeTruthy();
  });

  it("calls rename_cell and emits renamed on save", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    const wrapper = mount(CellDetailPanel, {
      props: { harnessId: "h1", cell, lockVersion: 0 },
    });
    await wrapper.find('[data-testid="name-input"]').setValue("new");
    await wrapper.find('[data-testid="name-save"]').trigger("click");
    await new Promise(r => setTimeout(r, 0));
    expect(invoke).toHaveBeenCalledWith("rename_cell", {
      harnessId: "h1", cellId: "c1", newName: "new", lockVersion: 0,
    });
    expect(wrapper.emitted("renamed")).toBeTruthy();
  });

  it("does not call rename_cell with empty name", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockClear();
    const wrapper = mount(CellDetailPanel, {
      props: { harnessId: "h1", cell, lockVersion: 0 },
    });
    await wrapper.find('[data-testid="name-input"]').setValue("   ");
    await wrapper.find('[data-testid="name-save"]').trigger("click");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("shows lock conflict message on the real backend lock_conflict code", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    // backend(commands.rs map_core_error)が実際に返す安定コードと同一文字列で検証する
    vi.mocked(invoke).mockRejectedValueOnce("lock_conflict");
    const wrapper = mount(CellDetailPanel, {
      props: { harnessId: "h1", cell, lockVersion: 0 },
    });
    await wrapper.find('[data-testid="name-input"]').setValue("new");
    await wrapper.find('[data-testid="name-save"]').trigger("click");
    await new Promise(r => setTimeout(r, 0));
    expect(wrapper.text()).toContain("他で編集が入りました。再読み込みしてください。");
  });

  it("keeps in-progress edit when same cell is replaced (polling)", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockClear();
    const wrapper = mount(CellDetailPanel, {
      props: { harnessId: "h1", cell: { ...cell }, lockVersion: 0 },
    });
    await wrapper.find('[data-testid="name-input"]').setValue("editing...");
    // ポーリングで detail が差し替わったのを模す: 同一 id の新オブジェクト
    await wrapper.setProps({ cell: { ...cell }, lockVersion: 1 });
    expect((wrapper.find('[data-testid="name-input"]').element as HTMLInputElement).value).toBe("editing...");
  });

  it("resets draft when switching to a different cell", async () => {
    const wrapper = mount(CellDetailPanel, {
      props: { harnessId: "h1", cell: { ...cell }, lockVersion: 0 },
    });
    await wrapper.find('[data-testid="name-input"]').setValue("editing...");
    const other = { id: "c2", name: "second", prompt: "p2", status: "active", terminal: false };
    await wrapper.setProps({ cell: other });
    expect((wrapper.find('[data-testid="name-input"]').element as HTMLInputElement).value).toBe("second");
  });
});
