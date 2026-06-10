// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// `Location` + `collectExternalProducerLocations` + `moveOperation`
// producer-ordering guards: an op carrying a classical-ref must
// land strictly after the M that produces the result. Covers the
// `Location.before` / `Location.inEarlierColumnThan` helpers that
// back the dropzone filter and the action-layer safety net that
// refuses drops violating the rule.

// @ts-check

import { test } from "node:test";
import assert from "node:assert/strict";
import { CircuitModel } from "../../../dist/ux/circuit-vis/data/circuitModel.js";
import {
  collectExternalProducerLocations,
  moveOperation,
} from "../../../dist/ux/circuit-vis/actions/circuitActions.js";
import { Location } from "../../../dist/ux/circuit-vis/data/location.js";

// ---------------------------------------------------------------------------
// Location helpers
// ---------------------------------------------------------------------------

test("Location.before: document-order comparison", () => {
  // Quick sanity tests for the helper that backs the
  // dropzone-filter and moveOperation safety-net.
  const L = (/** @type {string} */ s) => Location.parse(s);
  // Top-level columns.
  assert.equal(L("0,0").before(L("0,1")), true, "same col, smaller op first");
  assert.equal(L("0,1").before(L("0,0")), false, "same col, larger op last");
  assert.equal(L("0,0").before(L("1,0")), true, "smaller col first");
  assert.equal(L("1,0").before(L("0,0")), false, "larger col last");
  // Equal -> strict-before is false.
  assert.equal(
    L("0,1").before(L("0,1")),
    false,
    "equal is not strictly before",
  );
  // Ancestor / descendant.
  assert.equal(
    L("0,0").before(L("0,0-0,0")),
    true,
    "ancestor renders before descendant",
  );
  assert.equal(
    L("0,0-0,0").before(L("0,0")),
    false,
    "descendant does not come before ancestor",
  );
  // Cross-level comparison.
  assert.equal(
    L("0,0-5,5").before(L("0,1")),
    true,
    "deeply nested inside col 0 comes before col 1",
  );
  assert.equal(
    L("0,1").before(L("0,0-5,5")),
    false,
    "col 1 does not come before anything inside col 0",
  );
});

test("Location.inEarlierColumnThan: column-strict, ancestor-aware", () => {
  // Backs the dropzone-filter and moveOperation safety-net for the
  // "producer must precede consumer" rule. Different from plain
  // document-order `before`: two ops in the same column are
  // simultaneous, and ancestor groups project their column down
  // onto everything they contain.
  const L = (/** @type {string} */ s) => Location.parse(s);

  // Different top-level columns.
  assert.equal(
    L("0,0").inEarlierColumnThan(L("1,0")),
    true,
    "earlier top-level column",
  );
  assert.equal(
    L("1,0").inEarlierColumnThan(L("0,0")),
    false,
    "later top-level column",
  );

  // Same column, different op-index — different ops in the same
  // column are simultaneous, so neither is "earlier" than the other.
  assert.equal(
    L("0,0").inEarlierColumnThan(L("0,1")),
    false,
    "same col, different op is simultaneous",
  );
  assert.equal(
    L("0,1").inEarlierColumnThan(L("0,0")),
    false,
    "same col, different op is simultaneous (reverse)",
  );

  // Identical locations — strictly earlier is false.
  assert.equal(
    L("0,0").inEarlierColumnThan(L("0,0")),
    false,
    "identical is not strictly earlier",
  );

  // Ancestor vs descendant — both occupy the same outer column.
  assert.equal(
    L("0,0").inEarlierColumnThan(L("0,0-1,0")),
    false,
    "ancestor shares outer column with descendant",
  );
  assert.equal(
    L("0,0-1,0").inEarlierColumnThan(L("0,0")),
    false,
    "descendant shares outer column with ancestor",
  );

  // Same outer group, later inner column.
  assert.equal(
    L("0,0-1,0").inEarlierColumnThan(L("0,0-2,0")),
    true,
    "same outer group, later inner column",
  );

  // Same outer group, same inner column, different op-index.
  assert.equal(
    L("0,0-1,0").inEarlierColumnThan(L("0,0-1,1")),
    false,
    "same inner column is simultaneous",
  );
});

// ---------------------------------------------------------------------------
// collectExternalProducerLocations: only producers OUTSIDE the moved
// subtree count as constraints on where the consumer can land.
// ---------------------------------------------------------------------------

test("collectExternalProducerLocations: classical control with external M", () => {
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0, numResults: 1 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          // Top-level M produces (0,0).
          {
            kind: "measurement",
            gate: "M",
            qubits: [{ qubit: 0 }],
            results: [{ qubit: 0, result: 0 }],
          },
        ],
      },
      {
        components: [
          // Conditional unitary consumes (0,0).
          {
            kind: "unitary",
            gate: "if",
            isConditional: true,
            targets: [{ qubit: 0 }, { qubit: 0, result: 0 }],
            controls: [{ qubit: 0, result: 0 }],
            children: [
              {
                components: [
                  { kind: "unitary", gate: "H", targets: [{ qubit: 0 }] },
                ],
              },
            ],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  const producers = collectExternalProducerLocations(
    model.componentGrid,
    "1,0",
  );
  assert.deepEqual(
    producers,
    ["0,0"],
    "external producer M at top-level col 0 must be reported",
  );
});

test("collectExternalProducerLocations: internal producer M is excluded", () => {
  // Producer M lives INSIDE the moved subtree → it travels with
  // the consumer, so it imposes no drop-target constraint.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0, numResults: 1 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Group",
            targets: [{ qubit: 0 }],
            children: [
              {
                components: [
                  // Producer inside the group, child col 0.
                  {
                    kind: "measurement",
                    gate: "M",
                    qubits: [{ qubit: 0 }],
                    results: [{ qubit: 0, result: 0 }],
                  },
                ],
              },
              {
                components: [
                  // Consumer also inside the group, child col 1.
                  {
                    kind: "unitary",
                    gate: "X",
                    targets: [{ qubit: 0 }],
                    controls: [{ qubit: 0, result: 0 }],
                  },
                ],
              },
            ],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  const producers = collectExternalProducerLocations(
    model.componentGrid,
    "0,0",
  );
  assert.deepEqual(
    producers,
    [],
    "producer is internal to the moved subtree → not reported",
  );
});

// ---------------------------------------------------------------------------
// moveOperation: refuses drops that would violate producer-first ordering.
// ---------------------------------------------------------------------------

test("moveOperation: refuses dropping a conditional before its producer M", () => {
  // Dragging a classically-conditional unitary (or a group
  // containing one) to a column before its producing measurement
  // would leave classical refs pointing at registers that don't
  // exist yet at the consumer's position. moveOperation refuses
  // such drops (returns null, model untouched).
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0, numResults: 1 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          {
            kind: "measurement",
            gate: "M",
            qubits: [{ qubit: 0 }],
            results: [{ qubit: 0, result: 0 }],
          },
        ],
      },
      {
        components: [
          {
            kind: "unitary",
            gate: "if",
            isConditional: true,
            targets: [{ qubit: 0 }, { qubit: 0, result: 0 }],
            controls: [{ qubit: 0, result: 0 }],
            children: [
              {
                components: [
                  { kind: "unitary", gate: "H", targets: [{ qubit: 0 }] },
                ],
              },
            ],
          },
        ],
      },
    ],
  };
  const before = JSON.stringify(circuit);
  const model = new CircuitModel(circuit);

  // Try to drop the conditional at top-level col 0 with
  // insertNewColumn=true — would put consumer at col 0, pushing M
  // to col 1. Move must be refused.
  const result = moveOperation(model, "1,0", "0,0", 0, 0, false, true);

  assert.equal(result, null, "move must be refused");
  assert.equal(
    JSON.stringify({
      qubits: model.qubits,
      componentGrid: model.componentGrid,
    }),
    before,
    "refusal must not mutate the model",
  );
});

test("moveOperation: allows dropping a conditional AFTER its producer M", () => {
  // Boundary check: the same conditional dropped to a column
  // after the producer M must succeed. The refusal mustn't
  // over-trigger.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0, numResults: 1 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          {
            kind: "measurement",
            gate: "M",
            qubits: [{ qubit: 0 }],
            results: [{ qubit: 0, result: 0 }],
          },
        ],
      },
      // Unrelated filler column so we have somewhere to drop AFTER M.
      {
        components: [{ kind: "unitary", gate: "Y", targets: [{ qubit: 1 }] }],
      },
      {
        components: [
          {
            kind: "unitary",
            gate: "if",
            isConditional: true,
            targets: [{ qubit: 0 }, { qubit: 0, result: 0 }],
            controls: [{ qubit: 0, result: 0 }],
            children: [
              {
                components: [
                  { kind: "unitary", gate: "H", targets: [{ qubit: 0 }] },
                ],
              },
            ],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  // Drop the conditional at col 1 with insertNewColumn=true —
  // lands at new col 1 (after M at col 0). Must succeed.
  const result = moveOperation(model, "2,0", "1,0", 0, 0, false, true);

  assert.ok(result, "move must succeed: consumer remains after producer");
});

test("moveOperation: allows moving a group whose classical producer is INTERNAL", () => {
  // When the producer M lives inside the moved subtree, the
  // subtree is self-contained — there's no external constraint
  // and the move can go anywhere (including to the beginning of
  // the grid).
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0, numResults: 1 }, { id: 1 }],
    componentGrid: [
      // Filler column so col 0 is occupied.
      {
        components: [{ kind: "unitary", gate: "Y", targets: [{ qubit: 1 }] }],
      },
      {
        components: [
          {
            kind: "unitary",
            gate: "Group",
            targets: [{ qubit: 0 }],
            children: [
              {
                components: [
                  // Producer inside the group, child col 0.
                  {
                    kind: "measurement",
                    gate: "M",
                    qubits: [{ qubit: 0 }],
                    results: [{ qubit: 0, result: 0 }],
                  },
                ],
              },
              {
                components: [
                  // Consumer also inside the group, child col 1.
                  {
                    kind: "unitary",
                    gate: "X",
                    targets: [{ qubit: 0 }],
                    controls: [{ qubit: 0, result: 0 }],
                  },
                ],
              },
            ],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  // Drop the group at col 0 with insertNewColumn=true — would
  // normally be "before its producer", but since the producer is
  // internal, the move is allowed.
  const result = moveOperation(model, "1,0", "0,0", 0, 0, false, true);

  assert.ok(
    result,
    "move must succeed: internal producer travels with the consumer",
  );
});

test("moveOperation: refuses promoting a conditional to a sibling of the producer's outer group", () => {
  // The "promote-around-the-rule" scenario. Producer M lives
  // inside an outer group at top-level col 0; the consumer also
  // starts inside that group. Dragging the consumer OUT of the
  // group and dropping it as a sibling at top-level col 0 must
  // be refused — the consumer would land in the same top-level
  // time-step as the producer, even though it's a different op
  // position.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0, numResults: 1 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Outer",
            targets: [{ qubit: 0 }],
            children: [
              {
                components: [
                  // Producer at "0,0-0,0".
                  {
                    kind: "measurement",
                    gate: "M",
                    qubits: [{ qubit: 0 }],
                    results: [{ qubit: 0, result: 0 }],
                  },
                ],
              },
              {
                components: [
                  // Consumer at "0,0-1,0".
                  {
                    kind: "unitary",
                    gate: "if",
                    isConditional: true,
                    targets: [{ qubit: 0 }, { qubit: 0, result: 0 }],
                    controls: [{ qubit: 0, result: 0 }],
                    children: [
                      {
                        components: [
                          {
                            kind: "unitary",
                            gate: "H",
                            targets: [{ qubit: 0 }],
                          },
                        ],
                      },
                    ],
                  },
                ],
              },
            ],
          },
        ],
      },
    ],
  };
  const before = JSON.stringify({
    qubits: circuit.qubits,
    componentGrid: circuit.componentGrid,
  });
  const model = new CircuitModel(circuit);

  // Try to promote the consumer ("0,0-1,0") to a sibling of the
  // outer group at top-level col 0 ("0,X"). The target column is
  // the same as the outer group's column → simultaneous → refuse.
  const result = moveOperation(model, "0,0-1,0", "0,0", 0, 0, false, true);
  assert.equal(result, null, "must refuse: same top-level column as producer");
  assert.equal(
    JSON.stringify({
      qubits: model.qubits,
      componentGrid: model.componentGrid,
    }),
    before,
    "refusal must not mutate the model",
  );
});

test("moveOperation: allows promoting a conditional to a strictly later top-level column", () => {
  // Boundary check: the same promotion to top-level col 1 (strictly
  // after the producer's outer group at col 0) must succeed.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0, numResults: 1 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Outer",
            targets: [{ qubit: 0 }],
            children: [
              {
                components: [
                  {
                    kind: "measurement",
                    gate: "M",
                    qubits: [{ qubit: 0 }],
                    results: [{ qubit: 0, result: 0 }],
                  },
                ],
              },
              {
                components: [
                  {
                    kind: "unitary",
                    gate: "if",
                    isConditional: true,
                    targets: [{ qubit: 0 }, { qubit: 0, result: 0 }],
                    controls: [{ qubit: 0, result: 0 }],
                    children: [
                      {
                        components: [
                          {
                            kind: "unitary",
                            gate: "H",
                            targets: [{ qubit: 0 }],
                          },
                        ],
                      },
                    ],
                  },
                ],
              },
            ],
          },
        ],
      },
      // Filler at top-level col 1 so we have somewhere strictly
      // later than the outer group at col 0 to drop onto.
      {
        components: [{ kind: "unitary", gate: "Y", targets: [{ qubit: 1 }] }],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  // Promote consumer ("0,0-1,0") to top-level col 1 (after the
  // outer group). Must succeed.
  const result = moveOperation(model, "0,0-1,0", "1,0", 0, 0, false, true);
  assert.ok(result, "move must succeed: strictly later outer column");
});
