import { describe, it, expect, vi } from "vitest";
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

  it("calls data.onSelect with cellId when clicked", async () => {
    const onSelect = vi.fn();
    const wrapper = mount(CellNode, {
      props: {
        data: {
          cellId: "c1",
          name: "start",
          status: "active",
          terminal: false,
          isStart: true,
          onSelect,
        },
      },
      global: { stubs: { Handle: true } },
    });
    await wrapper.find('[data-testid="cell-node"]').trigger("click");
    expect(onSelect).toHaveBeenCalledWith("c1");
  });
});
