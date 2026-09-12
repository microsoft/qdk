// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import type { ValidatedMajoranaDevice } from "./types.js";

export type VirtualQubit = {
  id: number;
  cellId: number;
  cellRow: number;
  cellColumn: number;
  side: "left" | "right";
  physicalQubitIds: readonly [number, number];
};

export type VirtualAdjacencyEdge = {
  id: string;
  orientation: "horizontal" | "vertical";
  scope: "intra-cell" | "inter-cell";
  firstVirtualQubitId: number;
  secondVirtualQubitId: number;
  supportedOperation: "CX" | "CZ";
};

export type VirtualTopology = {
  device: ValidatedMajoranaDevice;
  qubits: readonly VirtualQubit[];
  adjacencyEdges: readonly VirtualAdjacencyEdge[];
  qubitsById: ReadonlyMap<number, VirtualQubit>;
  adjacencyById: ReadonlyMap<string, VirtualAdjacencyEdge>;
};

export function createVirtualTopology(
  device: ValidatedMajoranaDevice,
): VirtualTopology {
  const qubits = createVirtualQubits(device);
  const adjacencyEdges = createVirtualAdjacencyEdges(device);
  return {
    device,
    qubits,
    adjacencyEdges,
    qubitsById: new Map(qubits.map((qubit) => [qubit.id, qubit])),
    adjacencyById: new Map(adjacencyEdges.map((edge) => [edge.id, edge])),
  };
}

export function getVirtualQubitCount(device: ValidatedMajoranaDevice): number {
  return device.cellRows * device.cellColumns * 2;
}

export function getVirtualQubitId(
  cellRow: number,
  cellColumn: number,
  side: "left" | "right",
  device: ValidatedMajoranaDevice,
): number {
  if (
    !Number.isInteger(cellRow) ||
    !Number.isInteger(cellColumn) ||
    cellRow < 0 ||
    cellRow >= device.cellRows ||
    cellColumn < 0 ||
    cellColumn >= device.cellColumns
  ) {
    throw new RangeError(
      `Cell position (${cellRow}, ${cellColumn}) is outside the device`,
    );
  }
  const cellId = cellColumn * device.cellRows + cellRow;
  return 2 * cellId + (side === "right" ? 1 : 0);
}

export function getVirtualAdjacencyEdgeId(
  orientation: "horizontal" | "vertical",
  firstVirtualQubitId: number,
  secondVirtualQubitId: number,
): string {
  const [first, second] = normalizeEndpoints(
    firstVirtualQubitId,
    secondVirtualQubitId,
  );
  return `${orientation}-v${first}-v${second}`;
}

export function getVirtualAdjacency(
  device: ValidatedMajoranaDevice,
  firstVirtualQubitId: number,
  secondVirtualQubitId: number,
): VirtualAdjacencyEdge | undefined {
  assertVirtualQubitId(firstVirtualQubitId, device);
  assertVirtualQubitId(secondVirtualQubitId, device);
  if (firstVirtualQubitId === secondVirtualQubitId) {
    return undefined;
  }

  const first = getVirtualPosition(firstVirtualQubitId, device);
  const second = getVirtualPosition(secondVirtualQubitId, device);
  let orientation: VirtualAdjacencyEdge["orientation"] | undefined;
  let scope: VirtualAdjacencyEdge["scope"] | undefined;
  if (
    first.cellRow === second.cellRow &&
    first.cellColumn === second.cellColumn &&
    first.side !== second.side
  ) {
    orientation = "horizontal";
    scope = "intra-cell";
  } else if (
    first.cellRow === second.cellRow &&
    Math.abs(first.cellColumn - second.cellColumn) === 1 &&
    ((first.cellColumn < second.cellColumn &&
      first.side === "right" &&
      second.side === "left") ||
      (second.cellColumn < first.cellColumn &&
        second.side === "right" &&
        first.side === "left"))
  ) {
    orientation = "horizontal";
    scope = "inter-cell";
  } else if (
    first.cellColumn === second.cellColumn &&
    first.side === second.side &&
    Math.abs(first.cellRow - second.cellRow) === 1
  ) {
    orientation = "vertical";
    scope = "inter-cell";
  }

  if (orientation === undefined || scope === undefined) {
    return undefined;
  }
  return createVirtualAdjacencyEdge(
    orientation,
    scope,
    firstVirtualQubitId,
    secondVirtualQubitId,
  );
}

export function getVirtualAdjacencyEdge(
  topology: VirtualTopology,
  firstVirtualQubitId: number,
  secondVirtualQubitId: number,
): VirtualAdjacencyEdge | undefined {
  const adjacency = getVirtualAdjacency(
    topology.device,
    firstVirtualQubitId,
    secondVirtualQubitId,
  );
  return adjacency === undefined
    ? undefined
    : topology.adjacencyById.get(adjacency.id);
}

function createVirtualQubits(device: ValidatedMajoranaDevice): VirtualQubit[] {
  const qubits: VirtualQubit[] = [];
  for (let cellColumn = 0; cellColumn < device.cellColumns; cellColumn++) {
    for (let cellRow = 0; cellRow < device.cellRows; cellRow++) {
      const cellId = cellColumn * device.cellRows + cellRow;
      const firstPhysicalQubitId = 4 * cellId;
      qubits.push(
        {
          id: 2 * cellId,
          cellId,
          cellRow,
          cellColumn,
          side: "left",
          physicalQubitIds: [firstPhysicalQubitId, firstPhysicalQubitId + 2],
        },
        {
          id: 2 * cellId + 1,
          cellId,
          cellRow,
          cellColumn,
          side: "right",
          physicalQubitIds: [
            firstPhysicalQubitId + 1,
            firstPhysicalQubitId + 3,
          ],
        },
      );
    }
  }
  return qubits;
}

function createVirtualAdjacencyEdges(
  device: ValidatedMajoranaDevice,
): VirtualAdjacencyEdge[] {
  const edges: VirtualAdjacencyEdge[] = [];

  for (let cellColumn = 0; cellColumn < device.cellColumns; cellColumn++) {
    for (let cellRow = 0; cellRow < device.cellRows; cellRow++) {
      edges.push(
        createVirtualAdjacencyEdge(
          "horizontal",
          "intra-cell",
          getVirtualQubitId(cellRow, cellColumn, "left", device),
          getVirtualQubitId(cellRow, cellColumn, "right", device),
        ),
      );
    }
  }

  for (let cellColumn = 0; cellColumn < device.cellColumns; cellColumn++) {
    for (let cellRow = 0; cellRow + 1 < device.cellRows; cellRow++) {
      for (const side of ["left", "right"] as const) {
        edges.push(
          createVirtualAdjacencyEdge(
            "vertical",
            "inter-cell",
            getVirtualQubitId(cellRow, cellColumn, side, device),
            getVirtualQubitId(cellRow + 1, cellColumn, side, device),
          ),
        );
      }
    }
  }

  for (let cellColumn = 0; cellColumn + 1 < device.cellColumns; cellColumn++) {
    for (let cellRow = 0; cellRow < device.cellRows; cellRow++) {
      edges.push(
        createVirtualAdjacencyEdge(
          "horizontal",
          "inter-cell",
          getVirtualQubitId(cellRow, cellColumn, "right", device),
          getVirtualQubitId(cellRow, cellColumn + 1, "left", device),
        ),
      );
    }
  }

  return edges;
}

function createVirtualAdjacencyEdge(
  orientation: VirtualAdjacencyEdge["orientation"],
  scope: VirtualAdjacencyEdge["scope"],
  firstVirtualQubitId: number,
  secondVirtualQubitId: number,
): VirtualAdjacencyEdge {
  const [first, second] = normalizeEndpoints(
    firstVirtualQubitId,
    secondVirtualQubitId,
  );
  return {
    id: getVirtualAdjacencyEdgeId(orientation, first, second),
    orientation,
    scope,
    firstVirtualQubitId: first,
    secondVirtualQubitId: second,
    supportedOperation: orientation === "horizontal" ? "CX" : "CZ",
  };
}

function getVirtualPosition(
  virtualQubitId: number,
  device: ValidatedMajoranaDevice,
): {
  cellRow: number;
  cellColumn: number;
  side: "left" | "right";
} {
  const cellId = Math.floor(virtualQubitId / 2);
  return {
    cellRow: cellId % device.cellRows,
    cellColumn: Math.floor(cellId / device.cellRows),
    side: virtualQubitId % 2 === 0 ? "left" : "right",
  };
}

function assertVirtualQubitId(
  virtualQubitId: number,
  device: ValidatedMajoranaDevice,
): void {
  if (
    !Number.isInteger(virtualQubitId) ||
    virtualQubitId < 0 ||
    virtualQubitId >= getVirtualQubitCount(device)
  ) {
    throw new RangeError(
      `Virtual qubit ID ${virtualQubitId} is outside 0 through ${
        getVirtualQubitCount(device) - 1
      }`,
    );
  }
}

function normalizeEndpoints(
  first: number,
  second: number,
): readonly [number, number] {
  return first < second ? [first, second] : [second, first];
}
