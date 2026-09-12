// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import type { IslandRoute, ValidatedMajoranaDevice } from "./types.js";

export type MzmId = 1 | 2 | 3 | 4;
export type GridPosition = {
  row: number;
  column: number;
};

export type ScalableUnitCell = GridPosition & {
  id: number;
  tetronIds: readonly [number, number, number, number];
};

export type MzmNode = {
  id: string;
  mzmId: MzmId;
  qubitId: number;
  localPosition: "upper-left" | "upper-right" | "lower-left" | "lower-right";
};

export type Tetron = GridPosition & {
  id: number;
  cellId: number;
  cellRow: number;
  cellColumn: number;
  localNumber: 0 | 1 | 2 | 3;
  mzmNodes: readonly [MzmNode, MzmNode, MzmNode, MzmNode];
};

export type LinearIslandOwner =
  | {
      kind: "cell";
      cellId: number;
    }
  | {
      kind: "vertical-seam";
      upperCellId: number;
      lowerCellId: number;
    };

export type LinearIsland = {
  id: string;
  boundaryRow: number;
  tetronColumn: number;
  upperQubitId: number;
  lowerQubitId: number;
  owner: LinearIslandOwner;
};

export type AdjacencyEdge = {
  id: string;
  orientation: "horizontal" | "vertical";
  firstQubitId: number;
  secondQubitId: number;
};

export type MajoranaTopology = {
  device: ValidatedMajoranaDevice;
  cells: readonly ScalableUnitCell[];
  tetrons: readonly Tetron[];
  linearIslands: readonly LinearIsland[];
  adjacencyEdges: readonly AdjacencyEdge[];
  tetronsById: ReadonlyMap<number, Tetron>;
  islandsById: ReadonlyMap<string, LinearIsland>;
  adjacencyById: ReadonlyMap<string, AdjacencyEdge>;
};

const MZM_POSITIONS = [
  "upper-left",
  "upper-right",
  "lower-left",
  "lower-right",
] as const;

export function createMajoranaTopology(
  device: ValidatedMajoranaDevice,
): MajoranaTopology {
  const cells: ScalableUnitCell[] = [];
  const tetrons: Tetron[] = [];

  for (let cellColumn = 0; cellColumn < device.cellColumns; cellColumn++) {
    for (let cellRow = 0; cellRow < device.cellRows; cellRow++) {
      const cellId = cellColumn * device.cellRows + cellRow;
      const firstTetronId = cellId * 4;
      const tetronIds = [
        firstTetronId,
        firstTetronId + 1,
        firstTetronId + 2,
        firstTetronId + 3,
      ] as const;
      cells.push({ id: cellId, row: cellRow, column: cellColumn, tetronIds });

      for (let localNumber = 0; localNumber < 4; localNumber++) {
        const id = firstTetronId + localNumber;
        const row = cellRow * 2 + Math.floor(localNumber / 2);
        const column = cellColumn * 2 + (localNumber % 2);
        tetrons.push({
          id,
          cellId,
          cellRow,
          cellColumn,
          localNumber: localNumber as Tetron["localNumber"],
          row,
          column,
          mzmNodes: MZM_POSITIONS.map((localPosition, index) => ({
            id: getMzmNodeId(id, (index + 1) as MzmId),
            mzmId: (index + 1) as MzmId,
            qubitId: id,
            localPosition,
          })) as [MzmNode, MzmNode, MzmNode, MzmNode],
        });
      }
    }
  }

  const linearIslands = createLinearIslands(device);
  const adjacencyEdges = createAdjacencyEdges(device);

  return {
    device,
    cells,
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
  const cellBlock = Math.floor(qubitId / 4);
  const cellColumn = Math.floor(cellBlock / device.cellRows);
  const cellRow = cellBlock % device.cellRows;
  const localNumber = qubitId % 4;
  return {
    row: cellRow * 2 + Math.floor(localNumber / 2),
    column: cellColumn * 2 + (localNumber % 2),
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

  const cellRow = Math.floor(position.row / 2);
  const cellColumn = Math.floor(position.column / 2);
  const localNumber = (position.row % 2) * 2 + (position.column % 2);
  return (cellColumn * device.cellRows + cellRow) * 4 + localNumber;
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
        owner: createIslandOwner(boundaryRow, tetronColumn, device),
      });
    }
  }
  return islands;
}

function createIslandOwner(
  boundaryRow: number,
  tetronColumn: number,
  device: ValidatedMajoranaDevice,
): LinearIslandOwner {
  const cellColumn = Math.floor(tetronColumn / 2);
  const upperCellRow = Math.floor(boundaryRow / 2);
  if (boundaryRow % 2 === 0) {
    return {
      kind: "cell",
      cellId: cellColumn * device.cellRows + upperCellRow,
    };
  }
  return {
    kind: "vertical-seam",
    upperCellId: cellColumn * device.cellRows + upperCellRow,
    lowerCellId: cellColumn * device.cellRows + upperCellRow + 1,
  };
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
