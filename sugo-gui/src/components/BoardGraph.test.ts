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

  it("lays out branch+loop cells with numeric positions", () => {
    const cells = [
      { id: "c1", name: "start", status: "active", terminal: false },
      { id: "c2", name: "work", status: "active", terminal: false },
      { id: "c3", name: "review", status: "active", terminal: false },
      { id: "c4", name: "done", status: "active", terminal: true },
    ];
    const edges = [
      { from: "c1", to: "c2", label: "begin", guard: null },
      { from: "c2", to: "c3", label: "submit", guard: null },
      { from: "c3", to: "c2", label: "fail", guard: "不合格" },
      { from: "c3", to: "c4", label: "pass", guard: "合格" },
    ];
    const wrapper = mount(BoardGraph, {
      props: { cells, edges, startCellId: "c1" },
      global: { stubs: { VueFlow: true, Background: true, Controls: true, MiniMap: true } },
    });
    const vm = wrapper.vm as {
      nodes: { id: string; position: { x: number; y: number } }[];
      flowEdges: unknown[];
    };
    expect(vm.nodes.length).toBe(4);
    expect(vm.flowEdges.length).toBe(4);
    for (const n of vm.nodes) {
      expect(typeof n.position.x).toBe("number");
      expect(typeof n.position.y).toBe("number");
    }
  });

  it("passes cellId and onSelect into node data and emits select", () => {
    const wrapper = mount(BoardGraph, {
      props: { cells: sampleCells, edges: sampleEdges, startCellId: "c1" },
      global: { stubs: { VueFlow: true, Background: true, Controls: true, MiniMap: true } },
    });
    const vm = wrapper.vm as {
      nodes: { data: { cellId: string; onSelect: (id: string) => void } }[];
    };
    expect(vm.nodes[0].data.cellId).toBe("c1");
    vm.nodes[0].data.onSelect("c1");
    expect(wrapper.emitted("select")).toBeTruthy();
    expect(wrapper.emitted("select")![0]).toEqual(["c1"]);
  });
});
