<template>
  <div class="w-full h-[600px] bg-gray-50 rounded border">
    <VueFlow
      :nodes="nodes"
      :edges="flowEdges"
      :node-types="nodeTypes"
      fit-view-on-init
      class="w-full h-full"
    >
      <Background />
      <Controls />
      <MiniMap />
    </VueFlow>
  </div>
</template>

<script setup lang="ts">
import { computed, markRaw } from "vue";
import { VueFlow } from "@vue-flow/core";
import type { Node, Edge, NodeTypesObject } from "@vue-flow/core";
import { Background } from "@vue-flow/background";
import { Controls } from "@vue-flow/controls";
import { MiniMap } from "@vue-flow/minimap";
import dagre from "@dagrejs/dagre";
import CellNode from "./CellNode.vue";

interface CellData {
  id: string;
  name: string;
  status: string;
  terminal: boolean;
}
interface EdgeData {
  from: string;
  to: string;
  label: string;
  guard: string | null;
}

const props = defineProps<{
  cells: CellData[];
  edges: EdgeData[];
  startCellId: string;
}>();

const emit = defineEmits<{ select: [cellId: string] }>();

const nodeTypes = { cell: markRaw(CellNode) } as NodeTypesObject;

const NODE_W = 160;
const NODE_H = 60;

// dagre による自動レイアウト（左→右の階層配置）
const nodes = computed<Node[]>(() => {
  const g = new dagre.graphlib.Graph();
  g.setGraph({ rankdir: "LR", nodesep: 40, ranksep: 80 });
  g.setDefaultEdgeLabel(() => ({}));
  for (const c of props.cells) {
    g.setNode(c.id, { width: NODE_W, height: NODE_H });
  }
  for (const e of props.edges) {
    if (e.from && e.to) g.setEdge(e.from, e.to);
  }
  dagre.layout(g);

  return props.cells.map((c) => {
    const pos = g.node(c.id);
    return {
      id: c.id,
      type: "cell",
      position: { x: pos?.x ?? 0, y: pos?.y ?? 0 },
      data: {
        cellId: c.id,
        name: c.name,
        status: c.status,
        terminal: c.terminal,
        isStart: c.id === props.startCellId,
        onSelect: (id: string) => emit("select", id),
      },
    };
  });
});

const flowEdges = computed<Edge[]>(() =>
  props.edges.map((e, i) => ({
    id: `e-${i}`,
    source: e.from,
    target: e.to,
    label: e.guard ? `${e.label} [${e.guard}]` : e.label,
    animated: false,
  }))
);

// expose for tests
defineExpose({ nodes, flowEdges });
</script>
