// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import {
  MAJORANA_GEOMETRY,
  type AdjacencyGeometry,
  type MajoranaGeometry,
  type Point,
  type TetronGeometry,
} from "./geometry.js";
import type { OperationProjection } from "./operations.js";
import type { MajoranaTopology, MzmId } from "./topology.js";
import type { MajoranaTransitionPhase } from "./types.js";

export type TetronsProps = {
  topology: MajoranaTopology;
  geometry: MajoranaGeometry;
  operations: readonly OperationProjection[];
  showQubitLabels: boolean;
  showMzmLabels: boolean;
  transitionPhase: MajoranaTransitionPhase;
};

export function Tetrons({
  topology,
  geometry,
  operations,
  showQubitLabels,
  showMzmLabels,
  transitionPhase,
}: TetronsProps) {
  return (
    <g class="qs-majorana-tetrons" data-view="Tetrons">
      <g class="qs-majorana-topology" aria-label="Majorana device topology">
        {geometry.routingObstacles.map((obstacle) => (
          <rect
            key={obstacle.id}
            class={`qs-majorana-routing-obstacle qs-majorana-routing-obstacle-${obstacle.kind}`}
            data-majorana-id={obstacle.id}
            rx={MAJORANA_GEOMETRY.routingObstacleCornerRadius}
            ry={MAJORANA_GEOMETRY.routingObstacleCornerRadius}
            {...obstacle.bounds}
          />
        ))}
        {[...geometry.tetrons.values()].map((tetron) => (
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
        {[...geometry.linearIslands.values()].map((island) => (
          <g
            key={island.islandId}
            data-majorana-id={island.islandId}
            aria-label={`Linear island between qubits ${
              topology.islandsById.get(island.islandId)?.upperQubitId
            } and ${topology.islandsById.get(island.islandId)?.lowerQubitId}`}
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
        {topology.tetrons.flatMap((tetron) =>
          tetron.mzmNodes.map((node) => {
            const anchor = geometry.tetrons.get(tetron.id)?.mzmAnchors[
              node.mzmId
            ];
            if (anchor === undefined) {
              throw new Error(`Missing MZM anchor for ${node.id}`);
            }
            return (
              <circle
                key={node.id}
                class="qs-majorana-mzm"
                data-majorana-id={node.id}
                aria-label={`Qubit ${node.qubitId}, gamma ${node.mzmId}`}
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
          [...geometry.tetrons.values()].map((tetron) => (
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
          topology.tetrons.flatMap((tetron) =>
            tetron.mzmNodes.map((node) => {
              const anchor = geometry.tetrons.get(tetron.id)?.mzmAnchors[
                node.mzmId
              ];
              if (anchor === undefined) {
                throw new Error(`Missing MZM anchor for ${node.id}`);
              }
              return (
                <text
                  key={`label-${node.id}`}
                  class="qs-majorana-mzm-label"
                  x={anchor.x}
                  y={anchor.y - 12}
                >
                  γ{node.mzmId}
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
            key={`${operation.operation.name}-${index}`}
            class="qs-majorana-operation"
            aria-label={operationLabel(operation)}
            role="group"
          >
            <path
              class={`qs-majorana-loop ${
                operation.stroke === "dotted" ? "qs-majorana-loop-dotted" : ""
              }`}
              data-operation={operation.operation.name}
              data-stroke={operation.stroke}
              d={createLoopPath(operation, geometry)}
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
  geometry: AdjacencyGeometry;
  className: string;
}) {
  return (
    <line
      class={className}
      data-majorana-id={geometry.edgeId}
      x1={geometry.start.x}
      y1={geometry.start.y}
      x2={geometry.end.x}
      y2={geometry.end.y}
    />
  );
}

export function createLoopPath(
  operation: OperationProjection,
  geometry: MajoranaGeometry,
): string {
  const mzmNodes = operation.loopRoute.nodes.filter(
    (node) => node.kind === "mzm",
  );
  if (mzmNodes.length === 2) {
    const [first, second] = mzmNodes;
    const tetron = requiredTetronGeometry(geometry, first.qubitId);
    const points = tetronPath(tetron, first.mzmId, second.mzmId);
    const islandNode = operation.loopRoute.nodes.find(
      (node) => node.kind === "island",
    );
    if (islandNode === undefined) {
      points.push(...routeOutsideTetron(points.at(-1)!, points[0], tetron));
    } else {
      const island = geometry.linearIslands.get(islandNode.id);
      if (island === undefined) {
        throw new Error(`Missing geometry for island ${islandNode.id}`);
      }
      const end = points.at(-1)!;
      const islandEntry =
        Math.abs(end.x - island.start.x) < Math.abs(end.x - island.end.x)
          ? island.start
          : island.end;
      const islandExit =
        islandEntry === island.start ? island.end : island.start;
      const entryDirection: -1 | 1 = islandEntry === island.start ? -1 : 1;
      const exitDirection: -1 | 1 = entryDirection === -1 ? 1 : -1;
      points.push(
        ...routeOutside(points.at(-1)!, islandEntry, entryDirection),
        islandExit,
        ...routeOutside(islandExit, points[0], exitDirection),
      );
    }
    return pathFromPoints(points);
  }

  if (mzmNodes.length !== 4) {
    throw new Error(
      `Expected two or four MZM nodes, received ${mzmNodes.length}`,
    );
  }

  const firstTetron = requiredTetronGeometry(geometry, mzmNodes[0].qubitId);
  const secondTetron = requiredTetronGeometry(geometry, mzmNodes[2].qubitId);
  const firstPath = tetronPath(
    firstTetron,
    mzmNodes[0].mzmId,
    mzmNodes[1].mzmId,
  );
  let secondPath = tetronPath(
    secondTetron,
    mzmNodes[2].mzmId,
    mzmNodes[3].mzmId,
  );
  if (
    pointDistance(firstPath.at(-1)!, secondPath.at(-1)!) <
    pointDistance(firstPath.at(-1)!, secondPath[0])
  ) {
    secondPath = [...secondPath].reverse();
  }

  const orientation =
    operation.operation.kind === "joint"
      ? operation.operation.orientation
      : undefined;
  if (orientation === undefined) {
    throw new Error("Four-node loop is missing joint-operation metadata");
  }
  return pathFromPoints([
    ...firstPath,
    ...routeBetweenTetrons(
      firstPath.at(-1)!,
      secondPath[0],
      orientation,
      firstPath.at(-1)!.x < firstTetron.center.x ? -1 : 1,
    ),
    ...secondPath,
    ...routeBetweenTetrons(
      secondPath.at(-1)!,
      firstPath[0],
      orientation,
      secondPath.at(-1)!.x < secondTetron.center.x ? -1 : 1,
    ),
  ]);
}

function tetronPath(
  tetron: TetronGeometry,
  firstMzmId: MzmId,
  secondMzmId: MzmId,
): Point[] {
  const first = tetron.mzmAnchors[firstMzmId];
  const second = tetron.mzmAnchors[secondMzmId];
  if (
    (firstMzmId === 1 && secondMzmId === 2) ||
    (firstMzmId === 2 && secondMzmId === 1) ||
    (firstMzmId === 3 && secondMzmId === 4) ||
    (firstMzmId === 4 && secondMzmId === 3)
  ) {
    return [first, second];
  }
  return [
    first,
    { x: tetron.center.x, y: first.y },
    { x: tetron.center.x, y: second.y },
    second,
  ];
}

function routeOutsideTetron(
  from: Point,
  to: Point,
  tetron: TetronGeometry,
): Point[] {
  const direction = from.x < tetron.center.x ? -1 : 1;
  const outsideX =
    tetron.center.x +
    direction *
      (MAJORANA_GEOMETRY.tetronWidth / 2 +
        MAJORANA_GEOMETRY.routingBlockGap +
        MAJORANA_GEOMETRY.routingBlockSize / 2);
  return [{ x: outsideX, y: from.y }, { x: outsideX, y: to.y }, to];
}

function routeOutside(from: Point, to: Point, direction: -1 | 1): Point[] {
  const outsideX =
    from.x +
    direction *
      (MAJORANA_GEOMETRY.routingBlockGap +
        MAJORANA_GEOMETRY.routingBlockSize / 2);
  return [{ x: outsideX, y: from.y }, { x: outsideX, y: to.y }, to];
}

function routeBetweenTetrons(
  from: Point,
  to: Point,
  orientation: "horizontal" | "vertical",
  direction: -1 | 1,
): Point[] {
  if (orientation === "horizontal") {
    const channelX = (from.x + to.x) / 2;
    return [{ x: channelX, y: from.y }, { x: channelX, y: to.y }, to];
  }
  const channelX =
    from.x +
    direction *
      (MAJORANA_GEOMETRY.routingBlockGap +
        MAJORANA_GEOMETRY.routingBlockSize / 2);
  return [{ x: channelX, y: from.y }, { x: channelX, y: to.y }, to];
}

function requiredTetronGeometry(
  geometry: MajoranaGeometry,
  qubitId: number,
): TetronGeometry {
  const tetron = geometry.tetrons.get(qubitId);
  if (tetron === undefined) {
    throw new Error(`Missing geometry for qubit ${qubitId}`);
  }
  return tetron;
}

function pointDistance(first: Point, second: Point): number {
  return Math.hypot(first.x - second.x, first.y - second.y);
}

function pathFromPoints(points: readonly Point[]): string {
  const distinctPoints = points.filter(
    (point, index) =>
      index === 0 ||
      point.x !== points[index - 1].x ||
      point.y !== points[index - 1].y,
  );
  return `${distinctPoints
    .map((point, index) => `${index === 0 ? "M" : "L"} ${point.x} ${point.y}`)
    .join(" ")} Z`;
}

function operationLabel(operation: OperationProjection): string {
  const targets = operation.targetQubitIds.map((id) => `Q${id}`).join(" and ");
  const pulse = operation.stroke === "dotted" ? ", dotted T-pulse loop" : "";
  return `${operation.operation.name} on ${targets}${pulse}`;
}
