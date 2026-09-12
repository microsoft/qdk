// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import assert from "node:assert/strict";
import { test } from "node:test";
import {
  createMajoranaTopology,
  createVirtualOperationIdentity,
  createVirtualTopology,
  projectOperation,
  projectTraceStep,
  projectVirtualOperation,
  projectVirtualTraceStep,
  selectDisplayedVirtualOperations,
  validateMajoranaInput,
} from "../../dist/ux/majorana/index.js";

const validated = validateMajoranaInput([
  [2, 2],
  [
    [["Mx", [0], null]],
    [["My-up", [2], null]],
    [["My-lw", [0], null]],
    [["Mz-up", [2], null]],
    [["Mz-lw", [0], null]],
    [["T", [0], null]],
    [["Mzz", [0, 2], null]],
    [["Mzy", [0, 2], null]],
    [["Myy", [0, 2], null]],
    [["Myz", [0, 2], null]],
    [["Mxx", [0, 1], null]],
  ],
]);
const topology = createMajoranaTopology(validated.device);
const projections = validated.trace.map(([operation]) =>
  projectOperation(topology, operation),
);

test("projects all single-qubit MZM pairs and islands", () => {
  const expected = [
    ["Mx", ["q0-m2", "q0-m4"], undefined, "⟨X❘", "solid"],
    ["My-up", ["q2-m1", "q2-m4"], "island-r0-c0", "⟨Y❘", "solid"],
    ["My-lw", ["q0-m1", "q0-m4"], "island-r0-c0", "⟨Y❘", "solid"],
    ["Mz-up", ["q2-m1", "q2-m2"], "island-r0-c0", "⟨Z❘", "solid"],
    ["Mz-lw", ["q0-m3", "q0-m4"], "island-r0-c0", "⟨Z❘", "solid"],
    ["T", ["q0-m2", "q0-m4"], undefined, "T", "dotted"],
  ];

  projections.slice(0, 6).forEach((projection, index) => {
    const [name, mzmIds, islandId, overlay, stroke] = expected[index];
    assert.equal(projection.operation.name, name);
    assert.deepEqual(projection.participatingMzmNodeIds, mzmIds);
    assert.equal(projection.participatingIslandId, islandId);
    assert.equal(projection.overlays[0].value, overlay);
    assert.equal(projection.stroke, stroke);
  });
});

test("projects every joint operation without islands", () => {
  const expected = [
    ["Mzz", ["q0-m3", "q0-m4", "q2-m1", "q2-m2"], ["⟨Z❘", "⟨Z❘"]],
    ["Mzy", ["q0-m3", "q0-m4", "q2-m2", "q2-m3"], ["⟨Z❘", "⟨Y❘"]],
    ["Myy", ["q0-m1", "q0-m4", "q2-m1", "q2-m4"], ["⟨Y❘", "⟨Y❘"]],
    ["Myz", ["q0-m1", "q0-m4", "q2-m1", "q2-m2"], ["⟨Y❘", "⟨Z❘"]],
    ["Mxx", ["q0-m2", "q0-m4", "q1-m1", "q1-m3"], ["⟨X❘", "⟨X❘"]],
  ];

  projections.slice(6).forEach((projection, index) => {
    const [name, mzmIds, overlays] = expected[index];
    assert.equal(projection.operation.name, name);
    assert.deepEqual(projection.participatingMzmNodeIds, mzmIds);
    assert.deepEqual(
      projection.overlays.map((overlay) => overlay.value),
      overlays,
    );
    assert.equal(projection.participatingIslandId, undefined);
    assert.equal(projection.stroke, "solid");
    assert.ok(projection.activeAdjacencyEdgeId);
  });
});

test("projects complete concurrent steps without sharing mutable state", () => {
  const input = validateMajoranaInput([
    [1, 1],
    [
      [
        ["Mz-lw", [0], null],
        ["Mz-up", [3], null],
      ],
    ],
  ]);
  const fixtureTopology = createMajoranaTopology(input.device);
  const projected = projectTraceStep(fixtureTopology, input.trace[0]);

  assert.equal(projected.length, 2);
  assert.deepEqual(
    projected.map((projection) => projection.targetQubitIds),
    [[0], [3]],
  );
  assert.notEqual(projected[0].loopRoute.nodes, projected[1].loopRoute.nodes);
});

test("normalizes symmetric CZ and role-sensitive CX identities", () => {
  const operations = validatedVirtualOperations([
    ["CZ", [0, 2], 0],
    ["CZ", [2, 0], 0],
    ["CX", [0, 1], 0],
    ["CX", [1, 0], 0],
    ["T", [0], 0],
    ["T", [0], 5],
  ]);
  const identities = operations.map(createVirtualOperationIdentity);

  assert.deepEqual(identities[0], {
    key: '["CZ",[0,2]]',
    name: "CZ",
    normalizedTargetIds: [0, 2],
  });
  assert.deepEqual(identities[1], identities[0]);
  assert.notEqual(identities[2].key, identities[3].key);
  assert.deepEqual(identities[2].normalizedTargetIds, [0, 1]);
  assert.deepEqual(identities[3].normalizedTargetIds, [1, 0]);
  assert.deepEqual(identities[5], identities[4]);
});

test("projects the approved glyph for every virtual operation", () => {
  const operations = validatedVirtualOperations([
    ["T", [0], 0],
    ["H", [0], 0],
    ["S", [0], 0],
    ["Mx", [0], 0],
    ["My", [0], 0],
    ["Mz", [0], 0],
    ["CX", [0, 1], 0],
    ["CZ", [0, 2], 0],
  ]);
  const virtualTopology = createVirtualTopology(validated.device);
  const expectedGlyphs = [
    [{ virtualQubitId: 0, text: "T" }],
    [{ virtualQubitId: 0, text: "H" }],
    [{ virtualQubitId: 0, text: "S" }],
    [{ virtualQubitId: 0, text: "⟨X❘" }],
    [{ virtualQubitId: 0, text: "⟨Y❘" }],
    [{ virtualQubitId: 0, text: "⟨Z❘" }],
    [
      { virtualQubitId: 0, text: "●", role: "control" },
      { virtualQubitId: 1, text: "⨁", role: "target" },
    ],
    [
      { virtualQubitId: 0, text: "●", role: "participant" },
      { virtualQubitId: 2, text: "●", role: "participant" },
    ],
  ];

  operations.forEach((operation, index) => {
    assert.deepEqual(
      projectVirtualOperation(virtualTopology, operation).glyphs,
      expectedGlyphs[index],
    );
  });
});

test("projects both CX directions on intra-cell and inter-cell edges", () => {
  const operations = validatedVirtualOperations([
    ["CX", [0, 1], 0],
    ["CX", [1, 0], 0],
    ["CX", [1, 4], 0],
    ["CX", [4, 1], 0],
  ]);
  const virtualTopology = createVirtualTopology(validated.device);
  const projections = operations.map((operation) =>
    projectVirtualOperation(virtualTopology, operation),
  );

  assert.deepEqual(
    projections.map((projection) => projection.activeAdjacencyEdgeId),
    [
      "horizontal-v0-v1",
      "horizontal-v0-v1",
      "horizontal-v1-v4",
      "horizontal-v1-v4",
    ],
  );
  assert.deepEqual(
    projections.map((projection) =>
      projection.glyphs.map((glyph) => [
        glyph.virtualQubitId,
        glyph.text,
        glyph.role,
      ]),
    ),
    [
      [
        [0, "●", "control"],
        [1, "⨁", "target"],
      ],
      [
        [1, "●", "control"],
        [0, "⨁", "target"],
      ],
      [
        [1, "●", "control"],
        [4, "⨁", "target"],
      ],
      [
        [4, "●", "control"],
        [1, "⨁", "target"],
      ],
    ],
  );
});

test("projects both CZ orders with canonical glyph ordering", () => {
  const operations = validatedVirtualOperations([
    ["CZ", [0, 2], 0],
    ["CZ", [2, 0], 0],
  ]);
  const virtualTopology = createVirtualTopology(validated.device);
  const projections = operations.map((operation) =>
    projectVirtualOperation(virtualTopology, operation),
  );

  assert.equal(projections[0].activeAdjacencyEdgeId, "vertical-v0-v2");
  assert.equal(projections[1].activeAdjacencyEdgeId, "vertical-v0-v2");
  assert.deepEqual(projections[1].identity, projections[0].identity);
  assert.deepEqual(projections[1].glyphs, projections[0].glyphs);
});

test("rejects missing, nonadjacent, and orientation-mismatched topology", () => {
  const virtualTopology = createVirtualTopology(validated.device);
  const invalidOperations = [
    {
      kind: "single",
      name: "T",
      target: 99,
      precedence: 0,
    },
    {
      kind: "joint",
      name: "CX",
      control: 0,
      target: 2,
      precedence: 0,
      orientation: "horizontal",
      adjacencyEdgeId: "vertical-v0-v2",
    },
    {
      kind: "joint",
      name: "CX",
      control: 0,
      target: 5,
      precedence: 0,
      orientation: "horizontal",
      adjacencyEdgeId: "horizontal-v0-v5",
    },
    {
      kind: "joint",
      name: "CZ",
      firstTarget: 0,
      secondTarget: 1,
      precedence: 0,
      orientation: "vertical",
      adjacencyEdgeId: "horizontal-v0-v1",
    },
  ];

  for (const operation of invalidOperations) {
    assert.throws(
      () => projectVirtualOperation(virtualTopology, operation),
      /missing virtual qubit|no matching adjacency edge/,
    );
  }
});

test("coalesces duplicate metadata and retains every physical contributor", () => {
  const input = validateMajoranaInput(
    [
      [2, 2],
      [
        [
          ["Mx", [0], ["T", [0], 5]],
          ["T", [1], ["T", [0], 1]],
          ["Mzz", [0, 2], ["CZ", [0, 2], 3]],
          ["Myy", [0, 2], ["CZ", [2, 0], 2]],
          ["Mxx", [0, 1], ["CX", [0, 1], 0]],
          ["Mxx", [0, 1], ["CX", [1, 0], 0]],
        ],
      ],
    ],
    { enableVirtualView: true },
  );
  const virtualTopology = createVirtualTopology(input.device);
  const projected = projectVirtualTraceStep(virtualTopology, input.trace[0]);

  assert.equal(projected.length, 4);
  assert.deepEqual(projected[0].contributingPhysicalEvents, [
    { operation: "Mx", targetQubitIds: [0] },
    { operation: "T", targetQubitIds: [1] },
  ]);
  assert.equal(projected[0].operation.precedence, 1);
  assert.deepEqual(projected[1].contributingPhysicalEvents, [
    { operation: "Mzz", targetQubitIds: [0, 2] },
    { operation: "Myy", targetQubitIds: [0, 2] },
  ]);
  assert.equal(projected[1].operation.precedence, 2);
  assert.equal(projected[2].identity.key, '["CX",[0,1]]');
  assert.equal(projected[3].identity.key, '["CX",[1,0]]');
});

test("selects lower precedence and uses trace order to break ties", () => {
  const input = validateMajoranaInput(
    [
      [2, 2],
      [
        [
          ["Mx", [0], ["H", [0], 1]],
          ["T", [1], ["CX", [0, 1], 0]],
          ["Mx", [4], ["T", [4], 0]],
        ],
        [
          ["Mxx", [0, 1], ["CX", [0, 1], 1]],
          ["Mzz", [0, 2], ["CZ", [0, 2], 0]],
          ["Mx", [0], ["H", [0], 2]],
          ["T", [1], ["S", [1], 2]],
          ["Mx", [4], ["T", [4], 0]],
        ],
        [
          ["Mx", [0], ["H", [0], 0]],
          ["T", [1], ["S", [0], 0]],
          ["Mx", [4], ["T", [4], 0]],
        ],
      ],
    ],
    { enableVirtualView: true },
  );
  const virtualTopology = createVirtualTopology(input.device);

  const cxDisplayed = selectDisplayedVirtualOperations(
    projectVirtualTraceStep(virtualTopology, input.trace[0]),
  );
  assert.deepEqual(
    cxDisplayed.map(({ identity }) => identity.key),
    ['["CX",[0,1]]', '["T",[4]]'],
  );

  const czDisplayed = selectDisplayedVirtualOperations(
    projectVirtualTraceStep(virtualTopology, input.trace[1]),
  );
  assert.deepEqual(
    czDisplayed.map(({ identity }) => identity.key),
    ['["CZ",[0,2]]', '["T",[4]]'],
  );

  const tieDisplayed = selectDisplayedVirtualOperations(
    projectVirtualTraceStep(virtualTopology, input.trace[2]),
  );
  assert.deepEqual(
    tieDisplayed.map(({ identity }) => identity.key),
    ['["H",[0]]', '["T",[4]]'],
  );
});

function validatedVirtualOperations(entries) {
  return validateMajoranaInput(
    [
      [2, 2],
      entries.map((virtualOperation) => [["Mx", [0], virtualOperation]]),
    ],
    { enableVirtualView: true },
  ).trace.map(([operation]) => operation.virtualOperation);
}
