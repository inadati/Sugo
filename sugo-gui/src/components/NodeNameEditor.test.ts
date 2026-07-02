import { describe, it, expect } from "vitest";
import { mount } from "@vue/test-utils";
import NodeNameEditor from "./NodeNameEditor.vue";

const props = { initialName: "旧名", x: 100, y: 100, width: 150 };

describe("NodeNameEditor", () => {
  it("Enterキーは何も起きない（commitはemitされない）", async () => {
    const wrapper = mount(NodeNameEditor, { props });
    const input = wrapper.find("input");
    await input.setValue("  新名  ");
    await input.trigger("keydown", { key: "Enter" });
    expect(wrapper.emitted("commit")).toBeFalsy();
  });

  it("blurすると確定する（Enterは不要）", async () => {
    const wrapper = mount(NodeNameEditor, { props });
    const input = wrapper.find("input");
    await input.setValue("  新名  ");
    await input.trigger("blur");
    expect(wrapper.emitted("commit")?.[0]).toEqual(["新名"]);
  });

  it("does not commit empty name", async () => {
    const wrapper = mount(NodeNameEditor, { props });
    const input = wrapper.find("input");
    await input.setValue("   ");
    await input.trigger("blur");
    expect(wrapper.emitted("commit")).toBeFalsy();
  });

  it("emits cancel on Escape", async () => {
    const wrapper = mount(NodeNameEditor, { props });
    await wrapper.find("input").trigger("keydown", { key: "Escape" });
    expect(wrapper.emitted("cancel")).toBeTruthy();
  });

  it("does not commit (with old name) when Escape is followed by blur", async () => {
    const wrapper = mount(NodeNameEditor, { props });
    const input = wrapper.find("input");
    // 取消操作（Escape）→ アンマウント相当の blur が続いても commit は発火しない
    await input.trigger("keydown", { key: "Escape" });
    await input.trigger("blur");
    expect(wrapper.emitted("commit")).toBeFalsy();
    // cancel は1回だけ
    expect(wrapper.emitted("cancel")?.length).toBe(1);
  });

  it("commits only once even if blur fires twice", async () => {
    const wrapper = mount(NodeNameEditor, { props });
    const input = wrapper.find("input");
    await input.setValue("新名");
    await input.trigger("blur");
    await input.trigger("blur");
    expect(wrapper.emitted("commit")?.length).toBe(1);
  });
});
