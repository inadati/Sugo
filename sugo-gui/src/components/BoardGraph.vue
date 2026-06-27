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
import { VueFlow, Background, Controls, MiniMap } from "@vue-flow/core";
import type { Node, Edge } from "@vue-flow/core";
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

const nodeTypes = { cell: markRaw(CellNode) };

// グリッドレイアウト: 横 3列に並べる
const nodes = computed<Node[]>(() =>
  props.cells.map((c, i) => ({
    id: c.id,
    type: "cell",
    position: { x: (i % 3) * 220, y: Math.floor(i / 3) * 140 },
    data: {
      name: c.name,
      status: c.status,
      terminal: c.terminal,
      isStart: c.id === props.startCellId,
    },
  }))
);

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
