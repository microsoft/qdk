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
import {
  collectExternalProducerLocations,
  moveOperation,
} from "../../../dist/ux/circuit-vis/actions/circuitActions.js";
import { Location } from "../../../dist/ux/circuit-vis/data/location.js";
import { build, circuit, gate, group, meas, qubits } from "../_helpers.mjs";

// classically-conditional group: H@0 gated on classical reg 0:0.
const ifGroup = () =>
  group("if", [[gate("H", 0)]], { ctrls: [{ q: 0, r: 0 }], conditional: true });

/** Serialized snapshot of the mutable model state, for immutability checks. */
const snapshot = (/** @type {any} */ model) =>
  JSON.stringify({ qubits: model.qubits, componentGrid: model.componentGrid });

// ---------------------------------------------------------------------------
// Location helpers
// ---------------------------------------------------------------------------

test("Location.before: document-order comparison", () => {
  // Backs the dropzone-filter and moveOperation safety-net.
  // Each row: [a, b, a.before(b), why].
  /** @type {[string, string, boolean, string][]} */
  const cases = [
    ["0,0", "0,1", true, "same col, smaller op first"],
    ["0,1", "0,0", false, "same col, larger op last"],
    ["0,0", "1,0", true, "smaller col first"],
    ["1,0", "0,0", false, "larger col last"],
    ["0,1", "0,1", false, "equal is not strictly before"],
    ["0,0", "0,0-0,0", true, "ancestor renders before descendant"],
    ["0,0-0,0", "0,0", false, "descendant does not come before ancestor"],
    ["0,0-5,5", "0,1", true, "deeply nested in col 0 comes before col 1"],
    ["0,1", "0,0-5,5", false, "col 1 not before anything inside col 0"],
  ];
  for (const [a, b, want, why] of cases) {
    assert.equal(Location.parse(a).before(Location.parse(b)), want, why);
  }
});

test("Location.inEarlierColumnThan: column-strict, ancestor-aware", () => {
  // Backs the dropzone-filter and moveOperation safety-net for the
  // "producer must precede consumer" rule. Unlike document-order
  // `before`: two ops in the same column are simultaneous, and
  // ancestor groups project their column down onto everything they
  // contain. Each row: [a, b, a.inEarlierColumnThan(b), why].
  /** @type {[string, string, boolean, string][]} */
  const cases = [
    ["0,0", "1,0", true, "earlier top-level column"],
    ["1,0", "0,0", false, "later top-level column"],
    ["0,0", "0,1", false, "same col, different op is simultaneous"],
    ["0,1", "0,0", false, "same col, different op is simultaneous (reverse)"],
    ["0,0", "0,0", false, "identical is not strictly earlier"],
    ["0,0", "0,0-1,0", false, "ancestor shares outer column with descendant"],
    ["0,0-1,0", "0,0", false, "descendant shares outer column with ancestor"],
    ["0,0-1,0", "0,0-2,0", true, "same outer group, later inner column"],
    ["0,0-1,0", "0,0-1,1", false, "same inner column is simultaneous"],
  ];
  for (const [a, b, want, why] of cases) {
    assert.equal(
      Location.parse(a).inEarlierColumnThan(Location.parse(b)),
      want,
      why,
    );
  }
});

// ---------------------------------------------------------------------------
// collectExternalProducerLocations: only producers OUTSIDE the moved
// subtree count as constraints on where the consumer can land.
// ---------------------------------------------------------------------------

test("collectExternalProducerLocations: classical control with external M", () => {
  // M@0 produces (0,0); the conditional group at col 1 consumes it.
  const model = build(circuit(qubits(2, { 0: 1 }), [[meas(0)], [ifGroup()]]));
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
  const model = build(
    circuit(qubits(2, { 0: 1 }), [
      [
        group("Group", [
          [meas(0)],
          [gate("X", 0, { ctrls: [{ q: 0, r: 0 }] })],
        ]),
      ],
    ]),
  );
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
  // Dropping the conditional before its producing M would leave its
  // classical ref pointing at a not-yet-produced register.
  const model = build(circuit(qubits(2, { 0: 1 }), [[meas(0)], [ifGroup()]]));
  const before = snapshot(model);

  // drop the conditional at col 0 (insertNewColumn) → before M → refuse
  const result = moveOperation(model, "1,0", "0,0", 0, 0, false, true);

  assert.equal(result, null, "move must be refused");
  assert.equal(snapshot(model), before, "refusal must not mutate the model");
});

test("moveOperation: allows dropping a conditional AFTER its producer M", () => {
  // Boundary check: the refusal mustn't over-trigger for a drop
  // that lands strictly after the producer. Y@1 is filler.
  const model = build(
    circuit(qubits(2, { 0: 1 }), [[meas(0)], [gate("Y", 1)], [ifGroup()]]),
  );

  // drop the conditional at new col 1 (after M at col 0) → allowed
  const result = moveOperation(model, "2,0", "1,0", 0, 0, false, true);

  assert.ok(result, "move must succeed: consumer remains after producer");
});

test("moveOperation: allows moving a group whose classical producer is INTERNAL", () => {
  // Producer M lives inside the moved subtree → no external
  // constraint, so the move can go anywhere. Y@1 is filler.
  const model = build(
    circuit(qubits(2, { 0: 1 }), [
      [gate("Y", 1)],
      [
        group("Group", [
          [meas(0)],
          [gate("X", 0, { ctrls: [{ q: 0, r: 0 }] })],
        ]),
      ],
    ]),
  );

  // drop the group at col 0 (insertNewColumn) → allowed (internal M)
  const result = moveOperation(model, "1,0", "0,0", 0, 0, false, true);

  assert.ok(
    result,
    "move must succeed: internal producer travels with the consumer",
  );
});

test("moveOperation: refuses promoting a conditional to a sibling of the producer's outer group", () => {
  // Producer M and consumer both start inside Outer. Promoting the
  // consumer out to a top-level sibling at Outer's own column lands
  // it in the same time-step as the producer → refuse.
  const model = build(
    circuit(qubits(2, { 0: 1 }), [[group("Outer", [[meas(0)], [ifGroup()]])]]),
  );
  const before = snapshot(model);

  // promote consumer "0,0-1,0" to top-level col 0 (Outer's col) → refuse
  const result = moveOperation(model, "0,0-1,0", "0,0", 0, 0, false, true);
  assert.equal(result, null, "must refuse: same top-level column as producer");
  assert.equal(snapshot(model), before, "refusal must not mutate the model");
});

test("moveOperation: allows promoting a conditional to a strictly later top-level column", () => {
  // Boundary check: promotion to col 1 (strictly after Outer at
  // col 0) must succeed. Y@1 is filler.
  const model = build(
    circuit(qubits(2, { 0: 1 }), [
      [group("Outer", [[meas(0)], [ifGroup()]])],
      [gate("Y", 1)],
    ]),
  );

  // promote consumer "0,0-1,0" to top-level col 1 (after Outer) → allowed
  const result = moveOperation(model, "0,0-1,0", "1,0", 0, 0, false, true);
  assert.ok(result, "move must succeed: strictly later outer column");
});
