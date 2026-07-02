import { describe, it, expect, vi } from "vitest";
import { mount } from "@vue/test-utils";
import cytoscape from "cytoscape";
import BoardGraph from "./BoardGraph.vue";
import NodeNameEditor from "./NodeNameEditor.vue";

// cytoscape をモックし、登録された on ハンドラを記録して後からトリガできるようにする。
vi.mock("cytoscape", () => {
  const makeEl = (id: string) => ({
    length: 1,
    id: () => id,
    data: () => ({ source: "c1", target: "c2", origLabel: "next", origGuard: null }),
    position: vi.fn(),
    renderedPosition: () => ({ x: 0, y: 0 }),
    renderedWidth: () => 150,
    renderedHeight: () => 52,
    remove: vi.fn(),
    addClass: vi.fn(),
    removeClass: vi.fn(),
  });
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const fn: any = vi.fn(() => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const handlers: { ev: string; sel: string | undefined; cb: (...a: any[]) => void }[] = [];
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const cy: any = {
      _handlers: handlers,
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      on: (ev: string, a: any, b: any) => {
        if (typeof a === "function") handlers.push({ ev, sel: undefined, cb: a });
        else handlers.push({ ev, sel: a, cb: b });
      },
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      trigger: (ev: string, sel: string | undefined, ...args: any[]) => {
        handlers.filter((h) => h.ev === ev && h.sel === sel).forEach((h) => h.cb(...args));
      },
      elements: () => ({ remove: vi.fn() }),
      add: vi.fn(),
      nodes: () => ({ forEach: vi.fn(), filter: () => ({ boundingBox: () => ({ x1: 0, y1: 0, x2: 0, y2: 0 }) }) }),
      getElementById: (id: string) => makeEl(id),
      layout: () => ({ run: vi.fn(), one: vi.fn() }),
      edgehandles: () => cy.__eh,
      fit: vi.fn(),
      resize: vi.fn(),
      zoom: vi.fn(() => 1),
      center: vi.fn(),
      destroy: vi.fn(),
    };
    cy.__eh = {
      enableDrawMode: vi.fn(), disableDrawMode: vi.fn(),
      enable: vi.fn(), disable: vi.fn(), start: vi.fn(), stop: vi.fn(), destroy: vi.fn(),
    };
    fn.__lastCy = cy;
    return cy;
  });
  fn.use = vi.fn();
  return { default: fn };
});
vi.mock("cytoscape-dagre", () => ({ default: {} }));
vi.mock("cytoscape-edgehandles", () => ({ default: {} }));

const sampleCells = [
  { id: "c1", name: "start", status: "active", terminal: false },
  { id: "c2", name: "end", status: "active", terminal: true },
];
type EdgeInput = { from: string; to: string; label: string; guard: string | null };
const sampleEdges: EdgeInput[] = [{ from: "c1", to: "c2", label: "next", guard: null }];

type CellData = (typeof sampleCells)[0];
type El = { data: { id?: string; label?: string; origLabel?: string; origGuard?: string | null } };

// 直近に生成された cy モックを取得する。
// eslint-disable-next-line @typescript-eslint/no-explicit-any
function lastCy(): any {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return (cytoscape as any).__lastCy;
}

function mountGraph(edges: EdgeInput[] = sampleEdges) {
  return mount(BoardGraph, {
    props: { harnessId: "h1", cells: sampleCells, edges, startCellId: "c1" },
  });
}

describe("BoardGraph", () => {
  it("renders without crashing", () => {
    expect(mountGraph().exists()).toBe(true);
  });

  it("「全体表示」ボタンをクリックすると cy.fit が呼ばれる", async () => {
    const wrapper = mountGraph();
    const cy = lastCy();
    cy.fit.mockClear();
    await wrapper.find('[data-testid="fit-view"]').trigger("click");
    expect(cy.fit).toHaveBeenCalled();
  });

  it("adds START badge to start cell label", () => {
    const vm = mountGraph().vm as unknown as { buildLabel: (c: CellData) => string };
    expect(vm.buildLabel(sampleCells[0])).toContain("START");
    expect(vm.buildLabel(sampleCells[1])).not.toContain("START");
  });

  it("adds END badge to terminal cell label", () => {
    const vm = mountGraph().vm as unknown as { buildLabel: (c: CellData) => string };
    expect(vm.buildLabel(sampleCells[1])).toContain("END");
    expect(vm.buildLabel(sampleCells[0])).not.toContain("END");
  });

  it("uses plain label when guard is null", () => {
    const vm = mountGraph().vm as unknown as { buildElements: () => El[] };
    const edge = vm.buildElements().find((e) => e.data.id === "e-0");
    expect(edge?.data.label).toBe("next");
  });

  it("appends guard expression to edge label", () => {
    const edges = [{ from: "c1", to: "c2", label: "next", guard: "x > 0" }];
    const vm = mountGraph(edges).vm as unknown as { buildElements: () => El[] };
    const edge = vm.buildElements().find((e) => e.data.id === "e-0");
    expect(edge?.data.label).toBe("next [x > 0]");
  });

  it("carries original label and guard on edge data for deletion", () => {
    const edges = [{ from: "c1", to: "c2", label: "next", guard: "続ける" }];
    const vm = mountGraph(edges).vm as unknown as { buildElements: () => El[] };
    const edge = vm.buildElements().find((e) => e.data.id === "e-0");
    expect(edge?.data.origLabel).toBe("next");
    expect(edge?.data.origGuard).toBe("続ける");
  });

  // ── 接続フロー ────────────────────────────────────────────────────────
  it("emits connect with endpoints and screen coords on ehcomplete", () => {
    const wrapper = mountGraph();
    const cy = lastCy();
    const addedEdge = { remove: vi.fn() };
    cy.trigger(
      "ehcomplete",
      undefined,
      { originalEvent: { clientX: 10, clientY: 20 } },
      { id: () => "c1" },
      { id: () => "c2" },
      addedEdge,
    );
    expect(addedEdge.remove).toHaveBeenCalled();
    expect(wrapper.emitted("connect")?.[0]?.[0]).toEqual({ from: "c1", to: "c2", x: 10, y: 20 });
  });

  // ── Delete キー削除 ───────────────────────────────────────────────────
  it("emits nodeDelete when a selected node is deleted via Delete key", () => {
    const wrapper = mountGraph();
    const cy = lastCy();
    cy.trigger("tap", "node", { target: { id: () => "c1" } }); // ノードを選択
    wrapper.element.dispatchEvent(new KeyboardEvent("keydown", { key: "Delete", bubbles: true }));
    expect(wrapper.emitted("nodeDelete")?.[0]).toEqual(["c1"]);
  });

  it("emits edgeDelete with original label/guard when a selected edge is deleted", () => {
    const wrapper = mountGraph();
    const cy = lastCy();
    cy.trigger("tap", "edge", { target: { id: () => "e-0" } }); // エッジを選択
    wrapper.element.dispatchEvent(new KeyboardEvent("keydown", { key: "Backspace", bubbles: true }));
    expect(wrapper.emitted("edgeDelete")?.[0]?.[0]).toEqual({
      from: "c1", to: "c2", label: "next", guard: null,
    });
  });

  it("ignores Delete key while focus is in an input (inline editing)", () => {
    const wrapper = mountGraph();
    const cy = lastCy();
    cy.trigger("tap", "node", { target: { id: () => "c1" } }); // ノード選択済み
    const input = document.createElement("input");
    wrapper.element.appendChild(input);
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Delete", bubbles: true }));
    expect(wrapper.emitted("nodeDelete")).toBeFalsy();
  });

  it("does nothing on Delete key when nothing is selected", () => {
    const wrapper = mountGraph();
    wrapper.element.dispatchEvent(new KeyboardEvent("keydown", { key: "Delete", bubbles: true }));
    expect(wrapper.emitted("nodeDelete")).toBeFalsy();
    expect(wrapper.emitted("edgeDelete")).toBeFalsy();
  });

  // ── その場編集の起動 ───────────────────────────────────────────────────
  const fakeNode = (id: string) => ({
    id: () => id,
    renderedPosition: () => ({ x: 40, y: 60 }),
    renderedWidth: () => 150,
  });

  it("opens NodeNameEditor and emits nodeRename on double-click + commit", async () => {
    const wrapper = mountGraph();
    const cy = lastCy();
    cy.trigger("dbltap", "node", { target: fakeNode("c1") });
    await wrapper.vm.$nextTick();
    const editor = wrapper.findComponent(NodeNameEditor);
    expect(editor.exists()).toBe(true);
    editor.vm.$emit("commit", "新しい名前");
    expect(wrapper.emitted("nodeRename")?.[0]?.[0]).toEqual({ cellId: "c1", name: "新しい名前" });
  });

  it("emits edgeEdit with label/guard and screen coords on edge double-click", () => {
    const edges = [{ from: "c1", to: "c2", label: "next", guard: "続ける" }];
    const wrapper = mountGraph(edges);
    const cy = lastCy();
    cy.trigger("dbltap", "edge", {
      target: { data: () => ({ source: "c1", target: "c2", origLabel: "next", origGuard: "続ける" }) },
      renderedPosition: { x: 5, y: 7 },
    });
    expect(wrapper.emitted("edgeEdit")?.[0]?.[0]).toMatchObject({
      from: "c1", to: "c2", label: "next", guard: "続ける",
    });
  });

  it("shows connect handle on node mouseover and starts edge gesture on handle mousedown", async () => {
    const wrapper = mountGraph();
    const cy = lastCy();
    cy.trigger("mouseover", "node", { target: fakeNode("c1") });
    await wrapper.vm.$nextTick();
    const handle = wrapper.find('[data-testid="connect-handle"]');
    expect(handle.exists()).toBe(true);
    await handle.trigger("mousedown");
    expect(cy.__eh.start).toHaveBeenCalled();
  });
});
