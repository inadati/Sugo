import { describe, it, expect, vi } from "vitest";
import { mount } from "@vue/test-utils";
import CellDetailPanel from "./CellDetailPanel.vue";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue({ new_version: 2, lock_version: 1 }),
}));

const cell = { id: "c1", name: "old", prompt: "do the thing", status: "active", terminal: false, memo: "" };

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

  it("タイトル欄でEnterを押しても何も起きない（保存はフォーカスが外れたときのみ・IME変換確定Enterでの誤動作を避ける）", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockClear();
    const wrapper = mount(CellDetailPanel, {
      props: { harnessId: "h1", cell, lockVersion: 0 },
    });
    await wrapper.find('[data-testid="name-input"]').setValue("にほんご");
    await wrapper.find('[data-testid="name-input"]').trigger("keydown", { key: "Enter" });
    expect(invoke).not.toHaveBeenCalled();
    expect(wrapper.emitted("close")).toBeFalsy();
  });

  it("タイトル欄からフォーカスが外れると自動保存される（renamedのみemit、closeはしない）", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockClear();
    vi.mocked(invoke)
      .mockResolvedValueOnce({ new_version: 2, lock_version: 1 })
      .mockResolvedValueOnce({ new_version: 3, lock_version: 2 });
    const wrapper = mount(CellDetailPanel, {
      props: { harnessId: "h1", cell, lockVersion: 0 },
    });
    await wrapper.find('[data-testid="name-input"]').setValue("new");
    await wrapper.find('[data-testid="name-input"]').trigger("blur");
    await new Promise(r => setTimeout(r, 0));
    expect(invoke).toHaveBeenNthCalledWith(1, "rename_cell", {
      harnessId: "h1", cellId: "c1", newName: "new", lockVersion: 0,
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "set_cell_memo", {
      harnessId: "h1", cellId: "c1", memo: "", lockVersion: 1,
    });
    expect(wrapper.emitted("renamed")).toEqual([[3, 2]]);
    expect(wrapper.emitted("close")).toBeFalsy();
  });

  it("値が変わっていなければフォーカスが外れても保存しない", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockClear();
    const wrapper = mount(CellDetailPanel, {
      props: { harnessId: "h1", cell, lockVersion: 0 },
    });
    await wrapper.find('[data-testid="name-input"]').trigger("blur");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("does not call rename_cell with empty name", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockClear();
    const wrapper = mount(CellDetailPanel, {
      props: { harnessId: "h1", cell, lockVersion: 0 },
    });
    await wrapper.find('[data-testid="name-input"]').setValue("   ");
    await wrapper.find('[data-testid="name-input"]').trigger("blur");
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
    await wrapper.find('[data-testid="name-input"]').trigger("blur");
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
    const other = { id: "c2", name: "second", prompt: "p2", status: "active", terminal: false, memo: "" };
    await wrapper.setProps({ cell: other });
    expect((wrapper.find('[data-testid="name-input"]').element as HTMLInputElement).value).toBe("second");
  });

  it("shows delete button for a non-START active cell", () => {
    const wrapper = mount(CellDetailPanel, {
      props: { harnessId: "h1", cell, lockVersion: 0, isStart: false },
    });
    expect(wrapper.find('[data-testid="cell-delete"]').exists()).toBe(true);
  });

  it("hides delete button and calls delete_cell for START cell", () => {
    const wrapper = mount(CellDetailPanel, {
      props: { harnessId: "h1", cell, lockVersion: 0, isStart: true },
    });
    expect(wrapper.find('[data-testid="cell-delete"]').exists()).toBe(false);
    expect(wrapper.text()).toContain("START マスは削除できません。");
  });

  it("calls delete_cell and emits deleted on delete", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockClear();
    const wrapper = mount(CellDetailPanel, {
      props: { harnessId: "h1", cell, lockVersion: 0, isStart: false },
    });
    await wrapper.find('[data-testid="cell-delete"]').trigger("click");
    await new Promise((r) => setTimeout(r, 0));
    expect(invoke).toHaveBeenCalledWith("delete_cell", {
      harnessId: "h1", cellId: "c1", lockVersion: 0,
    });
    expect(wrapper.emitted("deleted")).toBeTruthy();
  });

  it("要望メモ欄からフォーカスが外れると set_cell_memo にその内容が渡る", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockClear();
    vi.mocked(invoke)
      .mockResolvedValueOnce({ new_version: 2, lock_version: 1 })
      .mockResolvedValueOnce({ new_version: 3, lock_version: 2 });
    const wrapper = mount(CellDetailPanel, {
      props: {
        harnessId: "h1",
        cell: { id: "c1", name: "n", prompt: "p", status: "active", terminal: false, memo: "" },
        lockVersion: 1,
      },
    });
    await wrapper.find('[data-testid="memo-input"]').setValue("直してほしい");
    await wrapper.find('[data-testid="memo-input"]').trigger("blur");
    await new Promise((r) => setTimeout(r, 0));

    expect(invoke).toHaveBeenNthCalledWith(2, "set_cell_memo", {
      harnessId: "h1",
      cellId: "c1",
      memo: "直してほしい",
      lockVersion: 1,
    });
    expect(wrapper.emitted("renamed")).toEqual([[3, 2]]);
    expect(wrapper.emitted("close")).toBeFalsy();
  });

  it("保存で lock_conflict が返るとエラーメッセージを表示しパネルは閉じない", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockRejectedValueOnce("lock_conflict");
    const wrapper = mount(CellDetailPanel, {
      props: {
        harnessId: "h1",
        cell: { id: "c1", name: "n", prompt: "p", status: "active", terminal: false, memo: "" },
        lockVersion: 1,
      },
    });
    await wrapper.find('[data-testid="memo-input"]').setValue("x");
    await wrapper.find('[data-testid="memo-input"]').trigger("blur");
    await new Promise((r) => setTimeout(r, 0));

    expect(wrapper.find('[data-testid="save-error"]').text()).toContain("他で編集が入りました");
    expect(wrapper.emitted("close")).toBeFalsy();
  });
});
