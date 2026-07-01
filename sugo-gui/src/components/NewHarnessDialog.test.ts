import { describe, it, expect, vi } from "vitest";
import { mount } from "@vue/test-utils";
import NewHarnessDialog from "./NewHarnessDialog.vue";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue({ harness_id: "new-id" }),
}));

describe("NewHarnessDialog", () => {
  it("emits close when cancel clicked", async () => {
    const wrapper = mount(NewHarnessDialog);
    await wrapper.find('[data-testid="cancel"]').trigger("click");
    expect(wrapper.emitted("close")).toBeTruthy();
  });

  it("does not submit with empty name", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockClear();
    const wrapper = mount(NewHarnessDialog);
    await wrapper.find('[data-testid="submit"]').trigger("click");
    expect(invoke).not.toHaveBeenCalled();
    expect(wrapper.text()).toContain("名前を入力してください。");
  });

  it("calls create_harness and emits created on success", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockClear();
    const wrapper = mount(NewHarnessDialog);
    await wrapper.find('[data-testid="name"]').setValue("  新ハーネス  ");
    await wrapper.find('[data-testid="desc"]').setValue("説明文");
    await wrapper.find('[data-testid="submit"]').trigger("click");
    await new Promise((r) => setTimeout(r, 0));
    expect(invoke).toHaveBeenCalledWith("create_harness", {
      name: "新ハーネス", description: "説明文",
    });
    expect(wrapper.emitted("created")?.[0]).toEqual(["new-id"]);
  });

  it("passes null description when empty", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockClear();
    const wrapper = mount(NewHarnessDialog);
    await wrapper.find('[data-testid="name"]').setValue("x");
    await wrapper.find('[data-testid="submit"]').trigger("click");
    await new Promise((r) => setTimeout(r, 0));
    expect(invoke).toHaveBeenCalledWith("create_harness", {
      name: "x", description: null,
    });
  });

  it("shows error message on failure", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockRejectedValueOnce("boom");
    const wrapper = mount(NewHarnessDialog);
    await wrapper.find('[data-testid="name"]').setValue("x");
    await wrapper.find('[data-testid="submit"]').trigger("click");
    await new Promise((r) => setTimeout(r, 0));
    expect(wrapper.text()).toContain("作成に失敗しました。");
    expect(wrapper.emitted("created")).toBeFalsy();
  });
});
