import { describe, it, expect, vi } from "vitest";
import { mount } from "@vue/test-utils";
import EdgeEditor from "./EdgeEditor.vue";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue({ new_version: 2, lock_version: 1 }),
}));

const addProps = {
  harnessId: "h1", mode: "add" as const, from: "c1", to: "c2",
  fromName: "開始", toName: "終了", lockVersion: 0,
};
const editProps = {
  harnessId: "h1", mode: "edit" as const, from: "c1", to: "c2",
  fromName: "開始", toName: "終了", oldLabel: "old",
  initialLabel: "old", initialGuard: "g", lockVersion: 3,
};

describe("EdgeEditor", () => {
  it("does not submit with empty label", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockClear();
    const wrapper = mount(EdgeEditor, { props: addProps });
    await wrapper.find('[data-testid="edge-submit"]').trigger("click");
    expect(invoke).not.toHaveBeenCalled();
    expect(wrapper.text()).toContain("ラベルを入力してください。");
  });

  it("add mode calls add_edge", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockClear();
    const wrapper = mount(EdgeEditor, { props: addProps });
    await wrapper.find('[data-testid="edge-label"]').setValue("次へ");
    await wrapper.find('[data-testid="edge-submit"]').trigger("click");
    await new Promise((r) => setTimeout(r, 0));
    expect(invoke).toHaveBeenCalledWith("add_edge", {
      harnessId: "h1", from: "c1", to: "c2", label: "次へ", guard: null, lockVersion: 0,
    });
    expect(wrapper.emitted("saved")).toBeTruthy();
  });

  it("edit mode calls update_edge with old_label", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockClear();
    const wrapper = mount(EdgeEditor, { props: editProps });
    await wrapper.find('[data-testid="edge-label"]').setValue("new");
    await wrapper.find('[data-testid="edge-guard"]').setValue("続ける");
    await wrapper.find('[data-testid="edge-submit"]').trigger("click");
    await new Promise((r) => setTimeout(r, 0));
    expect(invoke).toHaveBeenCalledWith("update_edge", {
      harnessId: "h1", from: "c1", to: "c2", oldLabel: "old",
      newLabel: "new", newGuard: "続ける", lockVersion: 3,
    });
  });

  it("shows duplicate message on duplicate_edge (add)", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockRejectedValueOnce("duplicate_edge");
    const wrapper = mount(EdgeEditor, { props: addProps });
    await wrapper.find('[data-testid="edge-label"]').setValue("次へ");
    await wrapper.find('[data-testid="edge-submit"]').trigger("click");
    await new Promise((r) => setTimeout(r, 0));
    expect(wrapper.text()).toContain("同じ経路・ラベルのエッジが既に存在します。");
  });

  it("emits reload (not inline message) on lock_conflict", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockRejectedValueOnce("lock_conflict");
    const wrapper = mount(EdgeEditor, { props: addProps });
    await wrapper.find('[data-testid="edge-label"]').setValue("次へ");
    await wrapper.find('[data-testid="edge-submit"]').trigger("click");
    await new Promise((r) => setTimeout(r, 0));
    expect(wrapper.emitted("reload")).toBeTruthy();
  });

  it("emits reload on edge_not_found (edit)", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockRejectedValueOnce("edge_not_found");
    const wrapper = mount(EdgeEditor, { props: editProps });
    await wrapper.find('[data-testid="edge-submit"]').trigger("click");
    await new Promise((r) => setTimeout(r, 0));
    expect(wrapper.emitted("reload")).toBeTruthy();
  });

  it("Enterキーは何も起きない（invokeが呼ばれない・ラベル欄）", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockClear();
    const wrapper = mount(EdgeEditor, { props: addProps });
    const input = wrapper.find('[data-testid="edge-label"]');
    await input.setValue("次へ");
    await input.trigger("keydown", { key: "Enter" });
    expect(invoke).not.toHaveBeenCalled();
  });

  it("Enterキーは何も起きない（invokeが呼ばれない・ガード欄）", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockClear();
    const wrapper = mount(EdgeEditor, { props: addProps });
    await wrapper.find('[data-testid="edge-label"]').setValue("次へ");
    const guardInput = wrapper.find('[data-testid="edge-guard"]');
    await guardInput.setValue("続ける");
    await guardInput.trigger("keydown", { key: "Enter" });
    expect(invoke).not.toHaveBeenCalled();
  });

  it("add モードでは削除ボタンが表示されない", () => {
    const wrapper = mount(EdgeEditor, { props: addProps });
    expect(wrapper.find('[data-testid="edge-delete"]').exists()).toBe(false);
  });

  it("edit モードで削除ボタンをクリックすると delete イベントを emit する", async () => {
    const wrapper = mount(EdgeEditor, { props: editProps });
    await wrapper.find('[data-testid="edge-delete"]').trigger("click");
    expect(wrapper.emitted("delete")).toEqual([[{ from: "c1", to: "c2", label: "old" }]]);
  });

  it("does not double-invoke when submit button is clicked repeatedly while submitting", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockClear();
    // invoke を保留させ、送信中にボタン連打しても再入しないことを確認する
    let resolve!: (v: { new_version: number; lock_version: number }) => void;
    vi.mocked(invoke).mockImplementationOnce(() => new Promise((r) => { resolve = r; }));
    const wrapper = mount(EdgeEditor, { props: addProps });
    await wrapper.find('[data-testid="edge-label"]').setValue("次へ");
    const submitBtn = wrapper.find('[data-testid="edge-submit"]');
    await submitBtn.trigger("click");
    await submitBtn.trigger("click");
    await submitBtn.trigger("click");
    expect(invoke).toHaveBeenCalledTimes(1);
    resolve({ new_version: 2, lock_version: 1 });
  });
});
