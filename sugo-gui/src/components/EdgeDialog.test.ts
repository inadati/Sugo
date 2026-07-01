import { describe, it, expect, vi } from "vitest";
import { mount } from "@vue/test-utils";
import EdgeDialog from "./EdgeDialog.vue";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue({ new_version: 2, lock_version: 1 }),
}));

const baseProps = {
  harnessId: "h1",
  from: "c1",
  to: "c2",
  fromName: "開始",
  toName: "終了",
  lockVersion: 0,
};

describe("EdgeDialog", () => {
  it("emits close when cancel clicked", async () => {
    const wrapper = mount(EdgeDialog, { props: baseProps });
    await wrapper.find('[data-testid="edge-cancel"]').trigger("click");
    expect(wrapper.emitted("close")).toBeTruthy();
  });

  it("does not submit with empty label", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockClear();
    const wrapper = mount(EdgeDialog, { props: baseProps });
    await wrapper.find('[data-testid="edge-submit"]').trigger("click");
    expect(invoke).not.toHaveBeenCalled();
    expect(wrapper.text()).toContain("ラベルを入力してください。");
  });

  it("calls add_edge with trimmed label and null guard, then emits added", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockClear();
    const wrapper = mount(EdgeDialog, { props: baseProps });
    await wrapper.find('[data-testid="edge-label"]').setValue("  次へ  ");
    await wrapper.find('[data-testid="edge-submit"]').trigger("click");
    await new Promise((r) => setTimeout(r, 0));
    expect(invoke).toHaveBeenCalledWith("add_edge", {
      harnessId: "h1", from: "c1", to: "c2", label: "次へ", guard: null, lockVersion: 0,
    });
    expect(wrapper.emitted("added")).toBeTruthy();
  });

  it("passes guard when provided", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockClear();
    const wrapper = mount(EdgeDialog, { props: baseProps });
    await wrapper.find('[data-testid="edge-label"]').setValue("進む");
    await wrapper.find('[data-testid="edge-guard"]').setValue("続ける");
    await wrapper.find('[data-testid="edge-submit"]').trigger("click");
    await new Promise((r) => setTimeout(r, 0));
    expect(invoke).toHaveBeenCalledWith("add_edge", {
      harnessId: "h1", from: "c1", to: "c2", label: "進む", guard: "続ける", lockVersion: 0,
    });
  });

  it("shows duplicate message when invoke rejects with duplicate_edge", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockRejectedValueOnce("duplicate_edge");
    const wrapper = mount(EdgeDialog, { props: baseProps });
    await wrapper.find('[data-testid="edge-label"]').setValue("次へ");
    await wrapper.find('[data-testid="edge-submit"]').trigger("click");
    await new Promise((r) => setTimeout(r, 0));
    expect(wrapper.text()).toContain("同じ経路・ラベルのエッジが既に存在します。");
    expect(wrapper.emitted("added")).toBeFalsy();
  });
});
