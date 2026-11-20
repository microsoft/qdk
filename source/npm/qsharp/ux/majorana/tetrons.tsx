// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { MAJORANA_GEOMETRY, type LineGeometry } from "./layout.js";
import type { OperationProjection } from "./operations.js";
import { MZM_IDS } from "./types.js";
import type { MajoranaViewProps } from "./app.js";

export function Tetrons({
  layout,
  operations,
  presentation: { showQubitLabels, showMzmLabels },
  transitionPhase,
}: MajoranaViewProps) {
  return (
    <g class="qs-majorana-tetrons" data-view="Tetrons">
      <g class="qs-majorana-topology" aria-label="Majorana device topology">
        {layout.routingObstacles.map((obstacle) => (
          <rect
            key={obstacle.id}
            class={`qs-majorana-routing-obstacle qs-majorana-routing-obstacle-${obstacle.kind}`}
            data-majorana-id={obstacle.id}
            rx={MAJORANA_GEOMETRY.routingObstacleCornerRadius}
            ry={MAJORANA_GEOMETRY.routingObstacleCornerRadius}
            {...obstacle.bounds}
          />
        ))}
        {layout.tetrons.map((tetron) => (
          <g
            key={`body-${tetron.qubitId}`}
            class="qs-majorana-tetron"
            data-majorana-id={`q${tetron.qubitId}`}
            data-center-x={tetron.center.x}
            data-center-y={tetron.center.y}
            aria-label={`Qubit ${tetron.qubitId} tetron`}
            role="group"
          >
            <TopologyLine
              geometry={tetron.upperRail}
              className="qs-majorana-tetron-rail"
            />
            <TopologyLine
              geometry={tetron.lowerRail}
              className="qs-majorana-tetron-rail"
            />
            <TopologyLine
              geometry={tetron.bridge}
              className="qs-majorana-tetron-bridge"
            />
          </g>
        ))}
        {[...layout.linearIslands.values()].map((island) => (
          <g
            key={island.id}
            data-majorana-id={island.id}
            aria-label={`Linear island between qubits ${island.upperQubitId} and ${island.lowerQubitId}`}
            role="group"
          >
            <line
              class="qs-majorana-island"
              x1={island.start.x}
              y1={island.start.y}
              x2={island.end.x}
              y2={island.end.y}
            />
            <circle
              class="qs-majorana-island-terminal"
              cx={island.start.x}
              cy={island.start.y}
              r="7"
            />
            <circle
              class="qs-majorana-island-terminal"
              cx={island.end.x}
              cy={island.end.y}
              r="7"
            />
          </g>
        ))}
      </g>
      <g class="qs-majorana-mzms" aria-label="Majorana zero modes">
        {layout.tetrons.flatMap((tetron) =>
          MZM_IDS.map((mzmId) => {
            const anchor = tetron.mzmAnchors[mzmId];
            const id = `q${tetron.qubitId}-m${mzmId}`;
            return (
              <circle
                key={id}
                class="qs-majorana-mzm"
                data-majorana-id={id}
                aria-label={`Qubit ${tetron.qubitId}, gamma ${mzmId}`}
                role="group"
                cx={anchor.x}
                cy={anchor.y}
                r="7"
              />
            );
          }),
        )}
      </g>
      <g class="qs-majorana-labels" aria-hidden="true">
        {showQubitLabels &&
          layout.tetrons.map((tetron) => (
            <text
              key={`label-q${tetron.qubitId}`}
              class="qs-majorana-qubit-label"
              x={tetron.center.x - 32}
              y={tetron.center.y + 5}
            >
              Q{tetron.qubitId}
            </text>
          ))}
        {showMzmLabels &&
          layout.tetrons.flatMap((tetron) =>
            MZM_IDS.map((mzmId) => {
              const anchor = tetron.mzmAnchors[mzmId];
              return (
                <text
                  key={`label-q${tetron.qubitId}-m${mzmId}`}
                  class="qs-majorana-mzm-label"
                  x={anchor.x}
                  y={anchor.y - 12}
                >
                  γ{mzmId}
                </text>
              );
            }),
          )}
      </g>
      <g
        class={`qs-majorana-operation-layer qs-majorana-operation-layer-${transitionPhase}`}
      >
        {operations.map((operation, index) => (
          <g
            key={`${operation.name}-${index}`}
            class="qs-majorana-operation"
            aria-label={operationLabel(operation)}
            role="group"
          >
            <path
              class={`qs-majorana-loop ${
                operation.stroke === "dotted" ? "qs-majorana-loop-dotted" : ""
              }`}
              data-operation={operation.name}
              data-stroke={operation.stroke}
              d={operation.loopPath}
            />
          </g>
        ))}
      </g>
    </g>
  );
}

function TopologyLine({
  geometry,
  className,
}: {
  geometry: LineGeometry;
  className: string;
}) {
  return (
    <line
      class={className}
      data-majorana-id={geometry.id}
      x1={geometry.start.x}
      y1={geometry.start.y}
      x2={geometry.end.x}
      y2={geometry.end.y}
    />
  );
}

function operationLabel(operation: OperationProjection): string {
  const targets = operation.overlays
    .map(({ qubitId }) => `Q${qubitId}`)
    .join(" and ");
  const pulse = operation.stroke === "dotted" ? ", dotted T-pulse loop" : "";
  return `${operation.name} on ${targets}${pulse}`;
}
