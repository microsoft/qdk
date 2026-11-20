// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { requiredTetron } from "./layout.js";
import type { MajoranaViewProps } from "./app.js";

export function Qubits({
  layout,
  operations,
  presentation: { showQubitLabels },
  transitionPhase,
}: MajoranaViewProps) {
  const activeQubits = new Set(
    operations.flatMap((operation) =>
      operation.overlays.map(({ qubitId }) => qubitId),
    ),
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
        {[...layout.adjacencyEdges.values()].map((edge) => {
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
              x1={edge.start.x}
              y1={edge.start.y}
              x2={edge.end.x}
              y2={edge.end.y}
            />
          );
        })}
      </g>
      <g class="qs-majorana-qubit-boxes" aria-label="Qubits">
        {layout.tetrons.map((tetron) => {
          const active = activeQubits.has(tetron.qubitId);
          return (
            <g
              key={tetron.qubitId}
              data-majorana-id={`q${tetron.qubitId}`}
              data-active={active ? "true" : "false"}
              aria-label={`Qubit ${tetron.qubitId}${active ? ", active" : ""}`}
              role="group"
            >
              <rect
                class="qs-majorana-qubit-box"
                rx="14"
                {...tetron.qubitBox}
              />
              {showQubitLabels && (
                <text
                  class="qs-majorana-qubit-box-label"
                  x={tetron.qubitBox.x + 10}
                  y={tetron.qubitBox.y + 20}
                  aria-hidden="true"
                >
                  Q{tetron.qubitId}
                </text>
              )}
            </g>
          );
        })}
      </g>
      <g
        class={`qs-majorana-operation-layer qs-majorana-operation-layer-${transitionPhase}`}
      >
        {[...layout.adjacencyEdges.values()]
          .filter((edge) => activeEdges.has(edge.id))
          .map((edge) => {
            const centerX = (edge.start.x + edge.end.x) / 2;
            const centerY = (edge.start.y + edge.end.y) / 2;
            return (
              <g key={`active-${edge.id}`} aria-hidden="true">
                <line
                  class="qs-majorana-adjacency-active"
                  x1={edge.start.x}
                  y1={edge.start.y}
                  x2={edge.end.x}
                  y2={edge.end.y}
                />
                <circle
                  class="qs-majorana-joint-measurement-background"
                  cx={centerX}
                  cy={centerY}
                  r="25"
                />
                <text
                  class="qs-majorana-joint-measurement-label"
                  x={centerX}
                  y={centerY}
                >
                  ⊗
                </text>
              </g>
            );
          })}
        {[...activeQubits].map((qubitId) => {
          const tetron = requiredTetron(layout, qubitId);
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
            const tetron = requiredTetron(layout, overlay.qubitId);
            return (
              <text
                key={`${operationIndex}-${overlay.qubitId}`}
                class={`qs-majorana-operation-overlay ${
                  operation.stroke === "dotted"
                    ? "qs-majorana-operation-overlay-pulse"
                    : "qs-majorana-operation-overlay-measurement"
                }`}
                data-operation={operation.name}
                data-qubit-id={overlay.qubitId}
                aria-label={`${operation.name} on Q${overlay.qubitId}: ${overlay.value}`}
                role="group"
                x={tetron.center.x}
                y={tetron.center.y}
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
