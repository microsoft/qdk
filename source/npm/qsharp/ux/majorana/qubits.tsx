// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import type { MajoranaGeometry } from "./geometry.js";
import type { OperationProjection } from "./operations.js";
import type { MajoranaTopology } from "./topology.js";
import type { MajoranaTransitionPhase } from "./types.js";

export type QubitsProps = {
  topology: MajoranaTopology;
  geometry: MajoranaGeometry;
  operations: readonly OperationProjection[];
  showQubitLabels: boolean;
  transitionPhase: MajoranaTransitionPhase;
};

export function Qubits({
  topology,
  geometry,
  operations,
  showQubitLabels,
  transitionPhase,
}: QubitsProps) {
  const activeQubits = new Set(
    operations.flatMap((operation) => [...operation.targetQubitIds]),
  );
  const activeEdges = new Set(
    operations.flatMap((operation) =>
      operation.activeAdjacencyEdgeId === undefined
        ? []
        : [operation.activeAdjacencyEdgeId],
    ),
  );

  return (
    <g class="qs-majorana-qubits" data-view="Qubits">
      <g class="qs-majorana-adjacencies" aria-label="Supported qubit adjacency">
        {topology.adjacencyEdges.map((edge) => {
          const line = geometry.adjacencyEdges.get(edge.id);
          if (line === undefined) {
            throw new Error(`Missing geometry for adjacency ${edge.id}`);
          }
          const active = activeEdges.has(edge.id);
          return (
            <line
              key={edge.id}
              class="qs-majorana-adjacency"
              data-majorana-id={edge.id}
              data-active={active ? "true" : "false"}
              aria-label={`${active ? "Active" : "Supported"} adjacency between Q${
                edge.firstQubitId
              } and Q${edge.secondQubitId}`}
              role="group"
              x1={line.start.x}
              y1={line.start.y}
              x2={line.end.x}
              y2={line.end.y}
            />
          );
        })}
      </g>
      <g class="qs-majorana-qubit-boxes" aria-label="Qubits">
        {topology.tetrons.map((tetron) => {
          const tetronGeometry = geometry.tetrons.get(tetron.id);
          if (tetronGeometry === undefined) {
            throw new Error(`Missing geometry for qubit ${tetron.id}`);
          }
          const active = activeQubits.has(tetron.id);
          return (
            <g
              key={tetron.id}
              data-majorana-id={`q${tetron.id}`}
              data-active={active ? "true" : "false"}
              aria-label={`Qubit ${tetron.id}${active ? ", active" : ""}`}
              role="group"
            >
              <rect
                class="qs-majorana-qubit-box"
                rx="14"
                {...tetronGeometry.qubitBox}
              />
              {showQubitLabels && (
                <text
                  class="qs-majorana-qubit-box-label"
                  x={tetronGeometry.qubitBox.x + 10}
                  y={tetronGeometry.qubitBox.y + 20}
                  aria-hidden="true"
                >
                  Q{tetron.id}
                </text>
              )}
            </g>
          );
        })}
      </g>
      <g
        class={`qs-majorana-operation-layer qs-majorana-operation-layer-${transitionPhase}`}
      >
        {topology.adjacencyEdges
          .filter((edge) => activeEdges.has(edge.id))
          .map((edge) => {
            const line = geometry.adjacencyEdges.get(edge.id);
            if (line === undefined) {
              throw new Error(`Missing geometry for adjacency ${edge.id}`);
            }
            return (
              <line
                key={`active-${edge.id}`}
                class="qs-majorana-adjacency-active"
                aria-hidden="true"
                x1={line.start.x}
                y1={line.start.y}
                x2={line.end.x}
                y2={line.end.y}
              />
            );
          })}
        {[...activeQubits].map((qubitId) => {
          const tetron = geometry.tetrons.get(qubitId);
          if (tetron === undefined) {
            throw new Error(`Missing geometry for qubit ${qubitId}`);
          }
          return (
            <g key={`active-q${qubitId}`} aria-hidden="true">
              <rect
                class="qs-majorana-qubit-box-active"
                rx="14"
                {...tetron.qubitBox}
              />
              {showQubitLabels && (
                <text
                  class="qs-majorana-active-qubit-label"
                  x={tetron.qubitBox.x + 10}
                  y={tetron.qubitBox.y + 20}
                >
                  Q{qubitId}
                </text>
              )}
            </g>
          );
        })}
        {operations.flatMap((operation, operationIndex) =>
          operation.overlays.map((overlay) => {
            const tetron = geometry.tetrons.get(overlay.qubitId);
            if (tetron === undefined) {
              throw new Error(`Missing geometry for qubit ${overlay.qubitId}`);
            }
            return (
              <text
                key={`${operationIndex}-${overlay.qubitId}`}
                class={`qs-majorana-operation-overlay ${
                  operation.stroke === "dotted"
                    ? "qs-majorana-operation-overlay-pulse"
                    : ""
                }`}
                data-operation={operation.operation.name}
                data-qubit-id={overlay.qubitId}
                aria-label={`${operation.operation.name} on Q${overlay.qubitId}: ${overlay.value}`}
                role="group"
                x={tetron.center.x}
                y={tetron.center.y + 7}
              >
                {overlay.value}
              </text>
            );
          }),
        )}
      </g>
    </g>
  );
}
