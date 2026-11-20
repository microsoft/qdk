// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import {
  SINGLE_OPERATIONS,
  JOINT_OPERATIONS,
  type Pauli,
  type MajoranaOperationName,
  type MzmPair,
  type Orientation,
  type ValidatedMajoranaOperation,
  type ValidatedMajoranaTraceStep,
} from "./types.js";
import {
  getAdjacencyEdgeId,
  getLinearIslandId,
  getTetronGridPosition,
  requiredTetron,
  MAJORANA_GEOMETRY,
  type LinearIsland,
  type MajoranaLayout,
  type Point,
  type TetronGeometry,
} from "./layout.js";

export type OperationOverlay = {
  qubitId: number;
  value: Pauli | "T";
};

export type OperationProjection = {
  name: MajoranaOperationName;
  overlays: readonly OperationOverlay[];
  activeAdjacencyEdgeId: string | undefined;
  loopPath: string;
  stroke: "solid" | "dotted";
};

export function projectOperation(
  layout: MajoranaLayout,
  operation: ValidatedMajoranaOperation,
): OperationProjection {
  if (operation.kind === "single") {
    const definition = SINGLE_OPERATIONS[operation.name];
    const tetron = requiredTetron(layout, operation.target);
    const island = resolveIsland(layout, operation);
    return {
      name: operation.name,
      overlays: [
        {
          qubitId: operation.target,
          value: definition.overlay,
        },
      ],
      activeAdjacencyEdgeId: undefined,
      loopPath: singleLoopPath(tetron, definition.mzmIds, island),
      stroke: definition.overlay === "T" ? "dotted" : "solid",
    };
  }

  const definition = JOINT_OPERATIONS[operation.name];
  const edgeId = getAdjacencyEdgeId(
    definition.orientation,
    operation.firstTarget,
    operation.secondTarget,
  );
  const edge = layout.adjacencyEdges.get(edgeId);
  if (edge === undefined) {
    throw new Error(
      `Validated operation ${operation.name} has no matching adjacency edge`,
    );
  }
  return {
    name: operation.name,
    overlays: [
      {
        qubitId: operation.firstTarget,
        value: definition.paulis[0],
      },
      {
        qubitId: operation.secondTarget,
        value: definition.paulis[1],
      },
    ],
    activeAdjacencyEdgeId: edge.id,
    loopPath: jointLoopPath(
      requiredTetron(layout, operation.firstTarget),
      requiredTetron(layout, operation.secondTarget),
      definition.mzmIds,
      edge.orientation,
    ),
    stroke: "solid",
  };
}

export function projectTraceStep(
  layout: MajoranaLayout,
  step: ValidatedMajoranaTraceStep,
): readonly OperationProjection[] {
  return step.map((operation) => projectOperation(layout, operation));
}

function resolveIsland(
  layout: MajoranaLayout,
  operation: Extract<ValidatedMajoranaOperation, { kind: "single" }>,
): LinearIsland | undefined {
  if (operation.islandRoute === "none") {
    return undefined;
  }
  const { row, column } = getTetronGridPosition(
    operation.target,
    layout.device,
  );
  const island = layout.linearIslands.get(
    getLinearIslandId(
      operation.islandRoute === "upper" ? row - 1 : row,
      column,
    ),
  );
  if (island === undefined) {
    throw new Error(
      `Validated operation ${operation.name} has no ${operation.islandRoute} island`,
    );
  }
  return island;
}

function singleLoopPath(
  tetron: TetronGeometry,
  mzmIds: MzmPair,
  island: LinearIsland | undefined,
): string {
  const { points, start, end } = tetronPath(tetron, mzmIds);
  if (island === undefined) {
    points.push(...routeOutside(end, start, end.x < tetron.center.x ? -1 : 1));
  } else {
    const islandEntry =
      pointDistance(end, island.start) < pointDistance(end, island.end)
        ? island.start
        : island.end;
    const islandExit = islandEntry === island.start ? island.end : island.start;
    const entryDirection = islandEntry === island.start ? -1 : 1;
    points.push(
      ...routeOutside(end, islandEntry, entryDirection),
      islandExit,
      ...routeOutside(islandExit, start, entryDirection === -1 ? 1 : -1),
    );
  }
  return pathFromPoints(points);
}

function jointLoopPath(
  firstTetron: TetronGeometry,
  secondTetron: TetronGeometry,
  pairs: readonly [MzmPair, MzmPair],
  orientation: Orientation,
): string {
  const firstPath = tetronPath(firstTetron, pairs[0]);
  let secondPath = tetronPath(secondTetron, pairs[1]);
  if (
    pointDistance(firstPath.end, secondPath.end) <
    pointDistance(firstPath.end, secondPath.start)
  ) {
    secondPath = {
      start: secondPath.end,
      end: secondPath.start,
      points: [...secondPath.points].reverse(),
    };
  }
  return pathFromPoints([
    ...firstPath.points,
    ...routeBetweenTetrons(
      firstPath.end,
      secondPath.start,
      orientation,
      firstPath.end.x < firstTetron.center.x ? -1 : 1,
    ),
    ...secondPath.points,
    ...routeBetweenTetrons(
      secondPath.end,
      firstPath.start,
      orientation,
      secondPath.end.x < secondTetron.center.x ? -1 : 1,
    ),
  ]);
}

function tetronPath(
  tetron: TetronGeometry,
  [firstMzmId, secondMzmId]: MzmPair,
): { start: Point; end: Point; points: Point[] } {
  const first = tetron.mzmAnchors[firstMzmId];
  const second = tetron.mzmAnchors[secondMzmId];
  return {
    start: first,
    end: second,
    points:
      first.y === second.y
        ? [first, second]
        : [
            first,
            { x: tetron.center.x, y: first.y },
            { x: tetron.center.x, y: second.y },
            second,
          ],
  };
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
  orientation: Orientation,
  direction: -1 | 1,
): Point[] {
  if (orientation === "vertical") {
    return routeOutside(from, to, direction);
  }
  const channelX = (from.x + to.x) / 2;
  return [{ x: channelX, y: from.y }, { x: channelX, y: to.y }, to];
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
