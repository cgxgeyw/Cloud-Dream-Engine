import { useEffect, useMemo, useState } from "react";
import ELK from "elkjs/lib/elk.bundled.js";
import {
  Background,
  Controls,
  Position,
  ReactFlow,
  type Edge,
  type Node,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import type { SessionMapEdge, SessionMapNode } from "../data/apiAdapter";

type SessionMapGraphProps = {
  nodes: SessionMapNode[];
  edges: SessionMapEdge[];
  compact?: boolean;
};

type MapNodeData = {
  label: string;
  current: boolean;
  discovered: boolean;
};

const elk = new ELK();
const NODE_WIDTH = 150;
const NODE_HEIGHT = 48;

export function SessionMapGraph({ nodes, edges, compact = false }: SessionMapGraphProps) {
  const [flowNodes, setFlowNodes] = useState<Node<MapNodeData>[]>([]);
  const currentNode = useMemo(() => nodes.find((node) => node.current) ?? null, [nodes]);

  const flowEdges = useMemo<Edge[]>(() => {
    const nodeIds = new Set(nodes.map((node) => node.node_id));
    return edges
      .filter((edge) => nodeIds.has(edge.source_node_id) && nodeIds.has(edge.target_node_id))
      .map((edge) => {
        const source = nodes.find((node) => node.node_id === edge.source_node_id);
        const target = nodes.find((node) => node.node_id === edge.target_node_id);
        return {
          id: edge.edge_id,
          source: edge.source_node_id,
          target: edge.target_node_id,
          animated: Boolean(source?.current || target?.current),
          type: "smoothstep",
          className: source?.current || target?.current ? "game-map-flow-edge game-map-flow-edge--current" : "game-map-flow-edge",
        };
      });
  }, [edges, nodes]);

  useEffect(() => {
    let cancelled = false;
    const layoutNodes = nodes.map((node) => ({
      id: node.node_id,
      width: NODE_WIDTH,
      height: NODE_HEIGHT,
    }));
    const layoutEdges = flowEdges.map((edge) => ({
      id: edge.id,
      sources: [edge.source],
      targets: [edge.target],
    }));

    void elk
      .layout({
        id: "session-map",
        layoutOptions: {
          "elk.algorithm": "layered",
          "elk.direction": "DOWN",
          "elk.spacing.nodeNode": "34",
          "elk.layered.spacing.nodeNodeBetweenLayers": "42",
          "elk.layered.nodePlacement.strategy": "BRANDES_KOEPF",
        },
        children: layoutNodes,
        edges: layoutEdges,
      })
      .then((graph) => {
        if (cancelled) {
          return;
        }
        const positionedNodes = nodes.map<Node<MapNodeData>>((node) => {
          const layoutNode = graph.children?.find((item) => item.id === node.node_id);
          return {
            id: node.node_id,
            type: "default",
            position: {
              x: layoutNode?.x ?? 0,
              y: layoutNode?.y ?? 0,
            },
            data: {
              label: node.label,
              current: node.current,
              discovered: node.discovered,
            },
            className: [
              "game-map-flow-node",
              node.current ? "game-map-flow-node--current" : "",
              node.discovered ? "" : "game-map-flow-node--undiscovered",
            ].filter(Boolean).join(" "),
            sourcePosition: Position.Bottom,
            targetPosition: Position.Top,
          };
        });
        setFlowNodes(positionedNodes);
      })
      .catch(() => {
        if (cancelled) {
          return;
        }
        setFlowNodes(
          nodes.map((node, index) => ({
            id: node.node_id,
            position: { x: 0, y: index * 76 },
            data: {
              label: node.label,
              current: node.current,
              discovered: node.discovered,
            },
            className: [
              "game-map-flow-node",
              node.current ? "game-map-flow-node--current" : "",
              node.discovered ? "" : "game-map-flow-node--undiscovered",
            ].filter(Boolean).join(" "),
          })),
        );
      });

    return () => {
      cancelled = true;
    };
  }, [flowEdges, nodes]);

  if (!nodes.length) {
    return <div className="game-card">当前地图暂无节点。</div>;
  }

  return (
    <div className={`game-map-graph${compact ? " game-map-graph--compact" : ""}`}>
      <div className="game-map-summary">
        <div className="game-map-summary-main">
          <strong>{currentNode?.label || "\u5c1a\u672a\u5b9a\u4f4d"}</strong>
          <span>{currentNode ? "\u5f53\u524d\u5730\u70b9" : "\u5c1a\u672a\u5b9a\u4f4d"}</span>
        </div>
      </div>
      <div className="game-map-flow-canvas" aria-label="地图拓扑图">
        <ReactFlow
          nodes={flowNodes}
          edges={flowEdges}
          fitView
          fitViewOptions={{ padding: 0.22 }}
          nodesDraggable={false}
          nodesConnectable={false}
          elementsSelectable={false}
          panOnDrag
          panOnScroll={false}
          zoomOnScroll
          proOptions={{ hideAttribution: true }}
        >
          <Background gap={18} size={1} className="game-map-flow-background" />
          <Controls showInteractive={false} position="bottom-right" />
        </ReactFlow>
      </div>
    </div>
  );
}
