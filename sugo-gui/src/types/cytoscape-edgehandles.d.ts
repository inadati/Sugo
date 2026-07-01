// cytoscape-edgehandles は型定義を同梱しないため、最小限の型を宣言する。
import "cytoscape";

declare module "cytoscape" {
  interface EdgehandlesInstance {
    enableDrawMode(): void;
    disableDrawMode(): void;
    enable(): void;
    disable(): void;
    start(sourceNode: NodeSingular): void;
    stop(): void;
    destroy(): void;
  }
  interface EdgehandlesOptions {
    canConnect?: (source: NodeSingular, target: NodeSingular) => boolean;
    edgeParams?: (source: NodeSingular, target: NodeSingular) => object;
    hoverDelay?: number;
    snap?: boolean;
    snapThreshold?: number;
    snapFrequency?: number;
    noEdgeEventsInDraw?: boolean;
    disableBrowserGestures?: boolean;
  }
  interface Core {
    edgehandles(options?: EdgehandlesOptions): EdgehandlesInstance;
  }
}
