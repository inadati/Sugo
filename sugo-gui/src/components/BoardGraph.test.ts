import { describe, it, expect } from "vitest";
import { mount } from "@vue/test-utils";
import BoardGraph from "./BoardGraph.vue";

const sampleCells = [
  { id: "c1", name: "start", status: "active", terminal: false },
  { id: "c2", name: "end", status: "active", terminal: true },
];
const sampleEdges = [
  { from: "c1", to: "c2", label: "next", guard: null },
];

describe("BoardGraph", () => {
  it("renders without crashing", () => {
    const wrapper = mount(BoardGraph, {
      props: { cells: sampleCells, edges: sampleEdges, startCellId: "c1" },
      global: {
        stubs: { VueFlow: true, Background: true, Controls: true, MiniMap: true },
      },
    });
    expect(wrapper.exists()).toBe(true);
  });

  it("creates a node per cell", () => {
    const wrapper = mount(BoardGraph, {
      props: { cells: sampleCells, edges: sampleEdges, startCellId: "c1" },
      global: {
        stubs: { VueFlow: true, Background: true, Controls: true, MiniMap: true },
      },
    });
    const vm = wrapper.vm as { nodes: { id: string; data: { isStart: boolean } }[] };
    expect(vm.nodes.length).toBe(2);
    expect(vm.nodes[0].data.isStart).toBe(true);
    expect(vm.nodes[1].data.isStart).toBe(false);
  });

  it("creates a flowEdge per edge with plain label when no guard", () => {
    const wrapper = mount(BoardGraph, {
      props: { cells: sampleCells, edges: sampleEdges, startCellId: "c1" },
      global: {
        stubs: { VueFlow: true, Background: true, Controls: true, MiniMap: true },
      },
    });
    const vm = wrapper.vm as { flowEdges: { id: string; source: string; target: string; label: string }[] };
    expect(vm.flowEdges.length).toBe(1);
    expect(vm.flowEdges[0].source).toBe("c1");
    expect(vm.flowEdges[0].target).toBe("c2");
    expect(vm.flowEdges[0].label).toBe("next");
  });

  it("appends guard expression to edge label when guard is present", () => {
    const edgesWithGuard = [{ from: "c1", to: "c2", label: "next", guard: "x > 0" }];
    const wrapper = mount(BoardGraph, {
      props: { cells: sampleCells, edges: edgesWithGuard, startCellId: "c1" },
      global: {
        stubs: { VueFlow: true, Background: true, Controls: true, MiniMap: true },
      },
    });
    const vm = wrapper.vm as { flowEdges: { label: string }[] };
    expect(vm.flowEdges[0].label).toBe("next [x > 0]");
  });
});
