// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import type { IslandRoute, ValidatedMajoranaDevice } from "./types.js";

export type MzmId = 1 | 2 | 3 | 4;
export type GridPosition = {
  row: number;
  column: number;
};

export type MzmNode = {
  id: string;
  mzmId: MzmId;
  qubitId: number;
};

export type Tetron = GridPosition & {
  id: number;
  mzmNodes: readonly [MzmNode, MzmNode, MzmNode, MzmNode];
};

export type LinearIsland = {
  id: string;
  boundaryRow: number;
  tetronColumn: number;
  upperQubitId: number;
  lowerQubitId: number;
};

export type AdjacencyEdge = {
  id: string;
  orientation: "horizontal" | "vertical";
  firstQubitId: number;
  secondQubitId: number;
};

export type MajoranaTopology = {
  device: ValidatedMajoranaDevice;
  tetrons: readonly Tetron[];
  linearIslands: readonly LinearIsland[];
  adjacencyEdges: readonly AdjacencyEdge[];
  tetronsById: ReadonlyMap<number, Tetron>;
  islandsById: ReadonlyMap<string, LinearIsland>;
  adjacencyById: ReadonlyMap<string, AdjacencyEdge>;
};

const MZM_IDS = [1, 2, 3, 4] as const;

export function createMajoranaTopology(
  device: ValidatedMajoranaDevice,
): MajoranaTopology {
  const tetrons: Tetron[] = [];

  for (let id = 0; id < device.qubitCount; id++) {
    const position = getTetronGridPosition(id, device);
    tetrons.push({
      id,
      ...position,
      mzmNodes: MZM_IDS.map((mzmId) => ({
        id: getMzmNodeId(id, mzmId),
        mzmId,
        qubitId: id,
      })) as [MzmNode, MzmNode, MzmNode, MzmNode],
    });
  }

  const linearIslands = createLinearIslands(device);
  const adjacencyEdges = createAdjacencyEdges(device);

  return {
    device,
    tetrons,
    linearIslands,
    adjacencyEdges,
    tetronsById: new Map(tetrons.map((tetron) => [tetron.id, tetron])),
    islandsById: new Map(linearIslands.map((island) => [island.id, island])),
    adjacencyById: new Map(adjacencyEdges.map((edge) => [edge.id, edge])),
  };
}

export function getTetronGridPosition(
  qubitId: number,
  device: ValidatedMajoranaDevice,
): GridPosition {
  assertQubitId(qubitId, device);
  return {
    row: Math.floor(qubitId / device.tetronColumns),
    column: qubitId % device.tetronColumns,
  };
}

export function getQubitIdAt(
  position: GridPosition,
  device: ValidatedMajoranaDevice,
): number {
  if (
    !Number.isInteger(position.row) ||
    !Number.isInteger(position.column) ||
    position.row < 0 ||
    position.row >= device.tetronRows ||
    position.column < 0 ||
    position.column >= device.tetronColumns
  ) {
    throw new RangeError(
      `Tetron position (${position.row}, ${position.column}) is outside the device`,
    );
  }

  return position.row * device.tetronColumns + position.column;
}

export function getMzmNodeId(qubitId: number, mzmId: MzmId): string {
  return `q${qubitId}-m${mzmId}`;
}

export function getLinearIslandId(
  boundaryRow: number,
  tetronColumn: number,
): string {
  return `island-r${boundaryRow}-c${tetronColumn}`;
}

export function getAdjacencyEdgeId(
  orientation: "horizontal" | "vertical",
  firstQubitId: number,
  secondQubitId: number,
): string {
  return `${orientation}-q${firstQubitId}-q${secondQubitId}`;
}

export function getAdjacentIsland(
  topology: MajoranaTopology,
  qubitId: number,
  route: Exclude<IslandRoute, "none">,
): LinearIsland | undefined {
  const position = getTetronGridPosition(qubitId, topology.device);
  const boundaryRow = route === "upper" ? position.row - 1 : position.row;
  if (boundaryRow < 0 || boundaryRow >= topology.device.tetronRows - 1) {
    return undefined;
  }
  return topology.islandsById.get(
    getLinearIslandId(boundaryRow, position.column),
  );
}

export function getAdjacencyEdge(
  topology: MajoranaTopology,
  orientation: "horizontal" | "vertical",
  firstQubitId: number,
  secondQubitId: number,
): AdjacencyEdge | undefined {
  return topology.adjacencyById.get(
    getAdjacencyEdgeId(orientation, firstQubitId, secondQubitId),
  );
}

function createLinearIslands(device: ValidatedMajoranaDevice): LinearIsland[] {
  const islands: LinearIsland[] = [];
  for (
    let boundaryRow = 0;
    boundaryRow < device.tetronRows - 1;
    boundaryRow++
  ) {
    for (
      let tetronColumn = 0;
      tetronColumn < device.tetronColumns;
      tetronColumn++
    ) {
      const upperQubitId = getQubitIdAt(
        { row: boundaryRow, column: tetronColumn },
        device,
      );
      const lowerQubitId = getQubitIdAt(
        { row: boundaryRow + 1, column: tetronColumn },
        device,
      );
      islands.push({
        id: getLinearIslandId(boundaryRow, tetronColumn),
        boundaryRow,
        tetronColumn,
        upperQubitId,
        lowerQubitId,
      });
    }
  }
  return islands;
}

function createAdjacencyEdges(
  device: ValidatedMajoranaDevice,
): AdjacencyEdge[] {
  const edges: AdjacencyEdge[] = [];
  for (let row = 0; row < device.tetronRows; row++) {
    for (let column = 0; column < device.tetronColumns; column++) {
      const firstQubitId = getQubitIdAt({ row, column }, device);
      if (column + 1 < device.tetronColumns) {
        const secondQubitId = getQubitIdAt({ row, column: column + 1 }, device);
        edges.push({
          id: getAdjacencyEdgeId("horizontal", firstQubitId, secondQubitId),
          orientation: "horizontal",
          firstQubitId,
          secondQubitId,
        });
      }
      if (row + 1 < device.tetronRows) {
        const secondQubitId = getQubitIdAt({ row: row + 1, column }, device);
        edges.push({
          id: getAdjacencyEdgeId("vertical", firstQubitId, secondQubitId),
          orientation: "vertical",
          firstQubitId,
          secondQubitId,
        });
      }
    }
  }
  return edges;
}

function assertQubitId(qubitId: number, device: ValidatedMajoranaDevice): void {
  if (
    !Number.isInteger(qubitId) ||
    qubitId < 0 ||
    qubitId >= device.qubitCount
  ) {
    throw new RangeError(
      `Qubit ID ${qubitId} is outside 0 through ${device.qubitCount - 1}`,
    );
  }
}
