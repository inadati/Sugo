import { describe, it, expect } from "vitest";
import { mount } from "@vue/test-utils";
import CellNode from "./CellNode.vue";

describe("CellNode", () => {
  it("shows cell name", () => {
    const wrapper = mount(CellNode, {
      props: { data: { name: "intro", status: "active", terminal: false, isStart: false } },
      global: { stubs: { Handle: true } },
    });
    expect(wrapper.text()).toContain("intro");
  });

  it("applies yellow background for draft cells", () => {
    const wrapper = mount(CellNode, {
      props: { data: { name: "draft-cell", status: "draft", terminal: false, isStart: false } },
      global: { stubs: { Handle: true } },
    });
    expect(wrapper.html()).toContain("bg-yellow");
  });

  it("shows terminal indicator for terminal cells", () => {
    const wrapper = mount(CellNode, {
      props: { data: { name: "end", status: "active", terminal: true, isStart: false } },
      global: { stubs: { Handle: true } },
    });
    expect(wrapper.text()).toContain("END");
  });

  it("shows start indicator for start cell", () => {
    const wrapper = mount(CellNode, {
      props: { data: { name: "start", status: "active", terminal: false, isStart: true } },
      global: { stubs: { Handle: true } },
    });
    expect(wrapper.html()).toContain("border-green");
  });
});
