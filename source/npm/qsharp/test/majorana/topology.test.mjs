// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import assert from "node:assert/strict";
import { test } from "node:test";
import {
  createVirtualTopology,
  getAdjacentIsland,
  getQubitIdAt,
  getTetronGridPosition,
  getVirtualAdjacency,
  getVirtualAdjacencyEdge,
  getVirtualQubitCount,
  getVirtualQubitId,
} from "../../dist/ux/majorana/index.js";
import {
  createFixture,
  createVirtualFixture,
  representativeDeviceSizes,
  representativeVirtualDeviceSizes,
} from "./fixtures.mjs";

test("generates expected topology counts for representative devices", () => {
  const expected = [
    {
      cells: 1,
      tetrons: 4,
      mzms: 16,
      islands: 2,
      horizontalEdges: 2,
      verticalEdges: 2,
    },
    {
      cells: 2,
      tetrons: 8,
      mzms: 32,
      islands: 6,
      horizontalEdges: 4,
      verticalEdges: 6,
    },
    {
      cells: 4,
      tetrons: 16,
      mzms: 64,
      islands: 12,
      horizontalEdges: 12,
      verticalEdges: 12,
    },
    {
      cells: 16,
      tetrons: 64,
      mzms: 256,
      islands: 56,
      horizontalEdges: 56,
      verticalEdges: 56,
    },
  ];

  representativeDeviceSizes.forEach(([rows, columns], index) => {
    const { topology } = createFixture(rows, columns);
    const horizontalEdges = topology.adjacencyEdges.filter(
      (edge) => edge.orientation === "horizontal",
    );
    const verticalEdges = topology.adjacencyEdges.filter(
      (edge) => edge.orientation === "vertical",
    );
    assert.equal(topology.cells.length, expected[index].cells);
    assert.equal(topology.tetrons.length, expected[index].tetrons);
    assert.equal(
      topology.tetrons.reduce(
        (count, tetron) => count + tetron.mzmNodes.length,
        0,
      ),
      expected[index].mzms,
    );
    assert.equal(topology.linearIslands.length, expected[index].islands);
    assert.equal(horizontalEdges.length, expected[index].horizontalEdges);
    assert.equal(verticalEdges.length, expected[index].verticalEdges);
  });
});

test("uses column-major cell blocks and row-major local tetron IDs", () => {
  const { topology } = createFixture(2, 2);
  const topRow = [0, 1, 8, 9].map(
    (qubitId) => topology.tetronsById.get(qubitId).row,
  );
  const topRowColumns = [0, 1, 8, 9].map(
    (qubitId) => topology.tetronsById.get(qubitId).column,
  );
  assert.deepEqual(topRow, [0, 0, 0, 0]);
  assert.deepEqual(topRowColumns, [0, 1, 2, 3]);

  const leftColumn = [0, 2, 4, 6].map(
    (qubitId) => topology.tetronsById.get(qubitId).column,
  );
  const leftColumnRows = [0, 2, 4, 6].map(
    (qubitId) => topology.tetronsById.get(qubitId).row,
  );
  assert.deepEqual(leftColumn, [0, 0, 0, 0]);
  assert.deepEqual(leftColumnRows, [0, 1, 2, 3]);
});

test("round-trips every qubit ID through its tetron-grid position", () => {
  const { topology } = createFixture(4, 4);
  for (let qubitId = 0; qubitId < topology.device.qubitCount; qubitId++) {
    const position = getTetronGridPosition(qubitId, topology.device);
    assert.equal(getQubitIdAt(position, topology.device), qubitId);
  }
});

test("rejects qubit IDs and positions outside the device", () => {
  const { topology } = createFixture(1, 1);
  assert.throws(() => getTetronGridPosition(4, topology.device), RangeError);
  assert.throws(
    () => getQubitIdAt({ row: 2, column: 0 }, topology.device),
    RangeError,
  );
});

test("assigns islands to cells and vertical cell seams", () => {
  const { topology } = createFixture(2, 1);
  const cellIslands = topology.linearIslands.filter(
    (island) => island.owner.kind === "cell",
  );
  const seamIslands = topology.linearIslands.filter(
    (island) => island.owner.kind === "vertical-seam",
  );

  assert.equal(cellIslands.length, 4);
  assert.equal(seamIslands.length, 2);
  assert.deepEqual(
    seamIslands.map((island) => [
      island.upperQubitId,
      island.lowerQubitId,
      island.owner.upperCellId,
      island.owner.lowerCellId,
    ]),
    [
      [2, 4, 0, 1],
      [3, 5, 0, 1],
    ],
  );
});

test("finds internal islands and omits perimeter islands", () => {
  const { topology } = createFixture(1, 1);
  assert.equal(getAdjacentIsland(topology, 0, "upper"), undefined);
  assert.equal(getAdjacentIsland(topology, 2, "lower"), undefined);
  assert.equal(getAdjacentIsland(topology, 0, "lower").id, "island-r0-c0");
  assert.equal(getAdjacentIsland(topology, 2, "upper").id, "island-r0-c0");
});

test("uses stable MZM identities and local positions", () => {
  const { topology } = createFixture(1, 1);
  assert.deepEqual(topology.tetronsById.get(3).mzmNodes, [
    {
      id: "q3-m1",
      mzmId: 1,
      qubitId: 3,
      localPosition: "upper-left",
    },
    {
      id: "q3-m2",
      mzmId: 2,
      qubitId: 3,
      localPosition: "upper-right",
    },
    {
      id: "q3-m3",
      mzmId: 3,
      qubitId: 3,
      localPosition: "lower-left",
    },
    {
      id: "q3-m4",
      mzmId: 4,
      qubitId: 3,
      localPosition: "lower-right",
    },
  ]);
});

test("looks up virtual IDs and undirected semantic adjacency", () => {
  const { input } = createFixture(2, 2);
  assert.equal(getVirtualQubitCount(input.device), 8);
  assert.equal(getVirtualQubitId(0, 0, "left", input.device), 0);
  assert.equal(getVirtualQubitId(1, 0, "right", input.device), 3);
  assert.equal(getVirtualQubitId(0, 1, "left", input.device), 4);

  assert.deepEqual(getVirtualAdjacency(input.device, 4, 1), {
    id: "horizontal-v1-v4",
    orientation: "horizontal",
    scope: "inter-cell",
    firstVirtualQubitId: 1,
    secondVirtualQubitId: 4,
    supportedOperation: "CX",
  });
  assert.deepEqual(getVirtualAdjacency(input.device, 2, 0), {
    id: "vertical-v0-v2",
    orientation: "vertical",
    scope: "inter-cell",
    firstVirtualQubitId: 0,
    secondVirtualQubitId: 2,
    supportedOperation: "CZ",
  });
  assert.equal(getVirtualAdjacency(input.device, 0, 5), undefined);
  assert.throws(() => getVirtualAdjacency(input.device, 0, 8), RangeError);
});

test("generates virtual qubits with fixed physical associations", () => {
  const { virtualTopology } = createVirtualFixture(2, 2);
  assert.deepEqual(virtualTopology.qubits, [
    {
      id: 0,
      cellId: 0,
      cellRow: 0,
      cellColumn: 0,
      side: "left",
      physicalQubitIds: [0, 2],
    },
    {
      id: 1,
      cellId: 0,
      cellRow: 0,
      cellColumn: 0,
      side: "right",
      physicalQubitIds: [1, 3],
    },
    {
      id: 2,
      cellId: 1,
      cellRow: 1,
      cellColumn: 0,
      side: "left",
      physicalQubitIds: [4, 6],
    },
    {
      id: 3,
      cellId: 1,
      cellRow: 1,
      cellColumn: 0,
      side: "right",
      physicalQubitIds: [5, 7],
    },
    {
      id: 4,
      cellId: 2,
      cellRow: 0,
      cellColumn: 1,
      side: "left",
      physicalQubitIds: [8, 10],
    },
    {
      id: 5,
      cellId: 2,
      cellRow: 0,
      cellColumn: 1,
      side: "right",
      physicalQubitIds: [9, 11],
    },
    {
      id: 6,
      cellId: 3,
      cellRow: 1,
      cellColumn: 1,
      side: "left",
      physicalQubitIds: [12, 14],
    },
    {
      id: 7,
      cellId: 3,
      cellRow: 1,
      cellColumn: 1,
      side: "right",
      physicalQubitIds: [13, 15],
    },
  ]);
  assert.equal(virtualTopology.qubitsById.get(6).side, "left");
});

test("generates exact virtual edge counts for representative devices", () => {
  for (const [cellRows, cellColumns] of representativeVirtualDeviceSizes) {
    const { virtualTopology } = createVirtualFixture(cellRows, cellColumns);
    const horizontal = virtualTopology.adjacencyEdges.filter(
      (edge) => edge.orientation === "horizontal",
    );
    const vertical = virtualTopology.adjacencyEdges.filter(
      (edge) => edge.orientation === "vertical",
    );
    assert.equal(virtualTopology.qubits.length, 2 * cellRows * cellColumns);
    assert.equal(
      horizontal.length,
      cellRows * cellColumns + cellRows * (cellColumns - 1),
    );
    assert.equal(vertical.length, 2 * (cellRows - 1) * cellColumns);
  }
});

test("matches the documented 2x2 virtual adjacency fixture", () => {
  const { virtualTopology } = createVirtualFixture(2, 2);
  const pairs = (orientation, scope) =>
    virtualTopology.adjacencyEdges
      .filter(
        (edge) =>
          edge.orientation === orientation &&
          (scope === undefined || edge.scope === scope),
      )
      .map((edge) => [edge.firstVirtualQubitId, edge.secondVirtualQubitId]);

  assert.deepEqual(pairs("horizontal", "intra-cell"), [
    [0, 1],
    [2, 3],
    [4, 5],
    [6, 7],
  ]);
  assert.deepEqual(pairs("horizontal", "inter-cell"), [
    [1, 4],
    [3, 6],
  ]);
  assert.deepEqual(pairs("vertical"), [
    [0, 2],
    [1, 3],
    [4, 6],
    [5, 7],
  ]);
  assert.ok(
    virtualTopology.adjacencyEdges
      .filter((edge) => edge.orientation === "horizontal")
      .every((edge) => edge.supportedOperation === "CX"),
  );
  assert.ok(
    virtualTopology.adjacencyEdges
      .filter((edge) => edge.orientation === "vertical")
      .every((edge) => edge.supportedOperation === "CZ"),
  );
});

test("looks up generated virtual adjacency by either endpoint order", () => {
  const { input } = createFixture(2, 2);
  const topology = createVirtualTopology(input.device);
  assert.equal(
    getVirtualAdjacencyEdge(topology, 4, 1),
    topology.adjacencyById.get("horizontal-v1-v4"),
  );
  assert.equal(getVirtualAdjacencyEdge(topology, 0, 5), undefined);
});
