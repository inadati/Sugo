import { describe, it, expect, vi } from "vitest";
import { mount } from "@vue/test-utils";
import BoardGraph from "./BoardGraph.vue";

vi.mock("cytoscape", () => {
  const fn = vi.fn().mockReturnValue({
    on: vi.fn(),
    elements: vi.fn().mockReturnValue({ remove: vi.fn() }),
    add: vi.fn(),
    layout: vi.fn().mockReturnValue({ run: vi.fn(), one: vi.fn() }),
    destroy: vi.fn(),
  });
  (fn as unknown as Record<string, unknown>).use = vi.fn();
  return { default: fn };
});
vi.mock("cytoscape-dagre", () => ({ default: {} }));

const sampleCells = [
  { id: "c1", name: "start", status: "active", terminal: false },
  { id: "c2", name: "end", status: "active", terminal: true },
];
const sampleEdges = [{ from: "c1", to: "c2", label: "next", guard: null }];

type CellData = (typeof sampleCells)[0];
type El = { data: { id?: string; label?: string; origLabel?: string; origGuard?: string | null } };

describe("BoardGraph", () => {
  it("renders without crashing", () => {
    const wrapper = mount(BoardGraph, {
      props: { harnessId: "h1", cells: sampleCells, edges: sampleEdges, startCellId: "c1" },
    });
    expect(wrapper.exists()).toBe(true);
  });

  it("adds START badge to start cell label", () => {
    const wrapper = mount(BoardGraph, {
      props: { harnessId: "h1", cells: sampleCells, edges: sampleEdges, startCellId: "c1" },
    });
    const vm = wrapper.vm as { buildLabel: (c: CellData) => string };
    expect(vm.buildLabel(sampleCells[0])).toContain("START");
    expect(vm.buildLabel(sampleCells[1])).not.toContain("START");
  });

  it("adds END badge to terminal cell label", () => {
    const wrapper = mount(BoardGraph, {
      props: { harnessId: "h1", cells: sampleCells, edges: sampleEdges, startCellId: "c1" },
    });
    const vm = wrapper.vm as { buildLabel: (c: CellData) => string };
    expect(vm.buildLabel(sampleCells[1])).toContain("END");
    expect(vm.buildLabel(sampleCells[0])).not.toContain("END");
  });

  it("uses plain label when guard is null", () => {
    const wrapper = mount(BoardGraph, {
      props: { harnessId: "h1", cells: sampleCells, edges: sampleEdges, startCellId: "c1" },
    });
    const vm = wrapper.vm as { buildElements: () => El[] };
    const edge = vm.buildElements().find((e) => e.data.id === "e-0");
    expect(edge?.data.label).toBe("next");
  });

  it("appends guard expression to edge label", () => {
    const edges = [{ from: "c1", to: "c2", label: "next", guard: "x > 0" }];
    const wrapper = mount(BoardGraph, {
      props: { harnessId: "h1", cells: sampleCells, edges, startCellId: "c1" },
    });
    const vm = wrapper.vm as { buildElements: () => El[] };
    const edge = vm.buildElements().find((e) => e.data.id === "e-0");
    expect(edge?.data.label).toBe("next [x > 0]");
  });

  it("carries original label and guard on edge data for deletion", () => {
    const edges = [{ from: "c1", to: "c2", label: "next", guard: "続ける" }];
    const wrapper = mount(BoardGraph, {
      props: { harnessId: "h1", cells: sampleCells, edges, startCellId: "c1" },
    });
    const vm = wrapper.vm as { buildElements: () => El[] };
    const edge = vm.buildElements().find((e) => e.data.id === "e-0");
    // 表示ラベルは装飾されるが、削除特定用の生データは元のまま
    expect(edge?.data.origLabel).toBe("next");
    expect(edge?.data.origGuard).toBe("続ける");
  });
});
