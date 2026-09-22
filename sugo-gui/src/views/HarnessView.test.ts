import { describe, it, expect, vi } from "vitest";
import { h } from "vue";
import { mount } from "@vue/test-utils";
import { createRouter, createMemoryHistory } from "vue-router";
import HarnessView from "./HarnessView.vue";

const mockDetail = vi.hoisted(() => ({
  harness_id: "h1", name: "my-harness", current_version: 1,
  lock_version: 0, has_draft: true, start_cell_id: "c1",
  cells: [
    { id: "c1", name: "start", prompt: "do the thing", status: "active", terminal: false, memo: "" },
    { id: "c2", name: "draft-one", prompt: "", status: "draft", terminal: true, memo: "" },
  ],
  edges: [],
  draft_diff: [{ cell_id: "c2", name: "draft-one", memo: "" }],
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(mockDetail),
}));

const relayoutAllMock = vi.fn(async () => {});
vi.mock("../components/BoardGraph.vue", () => ({
  default: {
    name: "BoardGraph",
    emits: ["select", "edge-edit", "edge-delete", "node-delete", "node-rename", "connect"],
    setup(_props: unknown, { expose }: { expose: (e: Record<string, unknown>) => void }) {
      expose({ relayoutAll: relayoutAllMock });
      return () => h("div");
    },
  },
}));
vi.mock("../components/AddCellDialog.vue", () => ({ default: { name: "AddCellDialog", template: "<div/>" } }));
vi.mock("../components/CellDetailPanel.vue", () => ({
  default: { name: "CellDetailPanel", props: ["harnessId", "cell", "lockVersion"], template: "<div class='panel'/>" },
}));

describe("HarnessView", () => {
  const makeRouter = () => createRouter({
    history: createMemoryHistory(),
    routes: [{ path: "/harness/:id", component: HarnessView, props: true }],
  });

  it("shows harness name", async () => {
    const router = makeRouter();
    await router.push("/harness/h1");
    const wrapper = mount(HarnessView, { props: { id: "h1" }, global: { plugins: [router] } });
    await new Promise(r => setTimeout(r, 0));
    expect(wrapper.text()).toContain("my-harness");
  });

  it("opens CellDetailPanel when a cell is selected", async () => {
    const router = makeRouter();
    const wrapper = mount(HarnessView, { props: { id: "h1" }, global: { plugins: [router] } });
    await new Promise(r => setTimeout(r, 0));
    wrapper.findComponent({ name: "BoardGraph" }).vm.$emit("select", "c1");
    await wrapper.vm.$nextTick();
    expect(wrapper.findComponent({ name: "CellDetailPanel" }).exists()).toBe(true);
  });

  it("EdgeEditorの削除ボタンでdelete_edgeを呼び、エディタを閉じる", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockClear();
    vi.mocked(invoke).mockResolvedValue(mockDetail);
    const router = makeRouter();
    const wrapper = mount(HarnessView, { props: { id: "h1" }, global: { plugins: [router] } });
    await new Promise((r) => setTimeout(r, 0));

    wrapper.findComponent({ name: "BoardGraph" }).vm.$emit("edge-edit", {
      from: "c1", to: "c2", label: "next", guard: null, x: 10, y: 10,
    });
    await wrapper.vm.$nextTick();

    vi.mocked(invoke).mockResolvedValueOnce({ new_version: 2, lock_version: 1 });
    await wrapper.find('[data-testid="edge-delete"]').trigger("click");
    await new Promise((r) => setTimeout(r, 0));

    expect(invoke).toHaveBeenCalledWith("delete_edge", {
      harnessId: "h1", from: "c1", to: "c2", label: "next", lockVersion: 0,
    });
    expect(wrapper.find('[data-testid="edge-delete"]').exists()).toBe(false);
  });

  it("エラー時にトーストを表示し3秒後に自動的に消える（useToastへの移行を確認）", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.useFakeTimers();
    vi.mocked(invoke).mockResolvedValue(mockDetail);
    const router = makeRouter();
    const wrapper = mount(HarnessView, { props: { id: "h1" }, global: { plugins: [router] } });
    await vi.advanceTimersByTimeAsync(0);

    vi.mocked(invoke).mockRejectedValueOnce(new Error("cannot_delete_start"));
    wrapper.findComponent({ name: "BoardGraph" }).vm.$emit("node-delete", "c1");
    await vi.advanceTimersByTimeAsync(0);
    expect(wrapper.get('[data-testid="toast"]').text()).toContain("START マスは削除できません");

    await vi.advanceTimersByTimeAsync(3000);
    await wrapper.vm.$nextTick();
    expect(wrapper.find('[data-testid="toast"]').exists()).toBe(false);

    vi.useRealTimers();
    vi.mocked(invoke).mockResolvedValue(mockDetail);
  });

  // ── 実行中ランの停止 ──────────────────────────────────────────────────
  //
  // 袋小路に入ったラン（出ている辺がどれも誤った前進になるセルで止まった状態）を
  // GUI から終わらせるための導線。invoke は get_harness / get_active_runs の2本を
  // 並列に呼ぶので、モックは呼び出し名で振り分ける。
  const activeRun = { run_id: "r1", current_cell_id: "c1", project_path: "/work/proj" };

  function mockWithRuns(invoke: ReturnType<typeof vi.fn>, runs: unknown[]) {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_active_runs") return Promise.resolve(runs);
      return Promise.resolve(mockDetail);
    });
  }

  it("実行中ランがあるとタブ名と現在のマス名を出した停止ボタンを表示する", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    mockWithRuns(invoke as never, [activeRun]);
    const router = makeRouter();
    const wrapper = mount(HarnessView, { props: { id: "h1" }, global: { plugins: [router] } });
    await new Promise((r) => setTimeout(r, 0));

    const banner = wrapper.get('[data-testid="running-run"]');
    expect(banner.text()).toContain("proj");
    expect(banner.text()).toContain("start");
    expect(wrapper.find('[data-testid="stop-run-btn"]').exists()).toBe(true);

    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockResolvedValue(mockDetail);
  });

  it("実行中ランが無いときは停止ボタンを出さない", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    mockWithRuns(invoke as never, []);
    const router = makeRouter();
    const wrapper = mount(HarnessView, { props: { id: "h1" }, global: { plugins: [router] } });
    await new Promise((r) => setTimeout(r, 0));

    expect(wrapper.find('[data-testid="stop-run-btn"]').exists()).toBe(false);

    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockResolvedValue(mockDetail);
  });

  it("停止ボタンは即座に止めず確認ダイアログを出す", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    mockWithRuns(invoke as never, [activeRun]);
    const router = makeRouter();
    const wrapper = mount(HarnessView, { props: { id: "h1" }, global: { plugins: [router] } });
    await new Promise((r) => setTimeout(r, 0));

    await wrapper.find('[data-testid="stop-run-btn"]').trigger("click");
    expect(wrapper.find('[data-testid="stop-run-dialog"]').exists()).toBe(true);
    expect(invoke).not.toHaveBeenCalledWith("stop_run", expect.anything());

    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockResolvedValue(mockDetail);
  });

  it("確認ダイアログで停止するとstop_runを呼び、キャンセルでは呼ばない", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    mockWithRuns(invoke as never, [activeRun]);
    const router = makeRouter();
    const wrapper = mount(HarnessView, { props: { id: "h1" }, global: { plugins: [router] } });
    await new Promise((r) => setTimeout(r, 0));

    await wrapper.find('[data-testid="stop-run-btn"]').trigger("click");
    await wrapper.find('[data-testid="stop-run-cancel-btn"]').trigger("click");
    expect(wrapper.find('[data-testid="stop-run-dialog"]').exists()).toBe(false);
    expect(invoke).not.toHaveBeenCalledWith("stop_run", expect.anything());

    await wrapper.find('[data-testid="stop-run-btn"]').trigger("click");
    await wrapper.find('[data-testid="stop-run-confirm-btn"]').trigger("click");
    await new Promise((r) => setTimeout(r, 0));

    expect(invoke).toHaveBeenCalledWith("stop_run", { runId: "r1" });
    // 停止後は一覧が空になるので、ボタンとダイアログの両方が消える
    mockWithRuns(invoke as never, []);
    expect(wrapper.find('[data-testid="stop-run-dialog"]').exists()).toBe(false);

    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockResolvedValue(mockDetail);
  });

  it("停止に失敗したらトーストで知らせ、ダイアログを閉じる", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_active_runs") return Promise.resolve([activeRun]);
      if (cmd === "stop_run") return Promise.reject(new Error("not found: run not found: r1"));
      return Promise.resolve(mockDetail);
    });
    const router = makeRouter();
    const wrapper = mount(HarnessView, { props: { id: "h1" }, global: { plugins: [router] } });
    await new Promise((r) => setTimeout(r, 0));

    await wrapper.find('[data-testid="stop-run-btn"]').trigger("click");
    await wrapper.find('[data-testid="stop-run-confirm-btn"]').trigger("click");
    await new Promise((r) => setTimeout(r, 0));

    expect(wrapper.get('[data-testid="toast"]').text()).toContain("停止");
    expect(wrapper.find('[data-testid="stop-run-dialog"]').exists()).toBe(false);

    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockResolvedValue(mockDetail);
  });

  it("整列ボタンを押すと BoardGraph の relayoutAll を呼ぶ", async () => {
    relayoutAllMock.mockClear();
    const router = makeRouter();
    const wrapper = mount(HarnessView, { props: { id: "h1" }, global: { plugins: [router] } });
    await new Promise((r) => setTimeout(r, 0));

    await wrapper.find('[data-testid="relayout"]').trigger("click");
    await new Promise((r) => setTimeout(r, 0));

    expect(relayoutAllMock).toHaveBeenCalled();
  });

  it("reloads detail when polled current_version changes", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.useFakeTimers();
    vi.mocked(invoke)
      .mockResolvedValueOnce({ ...mockDetail, current_version: 1 })
      .mockResolvedValue({ ...mockDetail, current_version: 2, lock_version: 1 });

    const router = makeRouter();
    const wrapper = mount(HarnessView, { props: { id: "h1" }, global: { plugins: [router] } });
    await vi.advanceTimersByTimeAsync(0);
    await vi.advanceTimersByTimeAsync(2000);
    const vm = wrapper.vm as unknown as { detail: { current_version: number } | null };
    expect(vm.detail?.current_version).toBe(2);
    vi.useRealTimers();
    vi.mocked(invoke).mockResolvedValue(mockDetail);
  });
});
