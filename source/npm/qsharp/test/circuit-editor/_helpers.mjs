// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// @ts-check
//
// Construction / extraction / assertion helpers for the
// `circuit-editor/circuit-actions/group*.test.mjs` suites.
//
// Leading underscore keeps this file out of the `**/*.test.mjs`
// discovery glob — it exports helpers, no tests of its own.
//
// Helpers return plain literal-shape objects (no validation, no
// class wrappers). They mirror what the model layer expects and
// have everything cast to `any` at the boundary so test bodies
// can stay free of `/** @type {any} */` ceremony.

import assert from "node:assert/strict";
import { CircuitModel } from "../../dist/ux/circuit-vis/data/circuitModel.js";
import { findOperation } from "../../dist/ux/circuit-vis/utils.js";

// ---------------------------------------------------------------
// Construction
// ---------------------------------------------------------------

/**
 * Build a qubits array.
 *
 * @param {number} n  number of qubits
 * @param {Record<number, number>} [results]  map from qubit index to
 *   `numResults`. Defaults to no `numResults` on any wire.
 * @returns {any[]}
 */
export const qubits = (n, results) =>
  Array.from({ length: n }, (_, i) =>
    results && results[i] !== undefined
      ? { id: i, numResults: results[i] }
      : { id: i },
  );

/**
 * Unitary op.
 *
 *   gate("H", 0)                              single-target
 *   gate("SWAP", [0, 2])                      multi-target
 *   gate("X", 1, { ctrls: [0] })              one quantum control
 *   gate("X", 1, { ctrls: [3, 4] })           two quantum controls
 *   gate("X", 1, { ctrls: [{ q: 0, r: 0 }],   one classical control
 *                  conditional: true })       (and conditional execution)
 *
 * @param {string} name
 * @param {number | number[]} target
 * @param {{ ctrls?: (number | { q: number, r?: number })[],
 *           conditional?: boolean }} [opts]
 * @returns {any}
 */
export const gate = (name, target, opts) => {
  const targets = (Array.isArray(target) ? target : [target]).map((q) => ({
    qubit: q,
  }));
  /** @type {any} */
  const out = { kind: "unitary", gate: name, targets };
  if (opts?.ctrls) {
    out.controls = opts.ctrls.map((c) =>
      typeof c === "number" ? { qubit: c } : { qubit: c.q, result: c.r ?? 0 },
    );
  }
  if (opts?.conditional) out.isConditional = true;
  return out;
};

/**
 * Measurement op.
 *
 *   meas(2)                       // M on q2 producing result 0
 *   meas(2, { result: 1 })        // M on q2 producing result 1
 *   meas(2, { gate: "Measure" })  // customize gate name
 *
 * @param {number} qubit
 * @param {{ gate?: string, result?: number }} [opts]
 * @returns {any}
 */
export const meas = (qubit, opts) => ({
  kind: "measurement",
  gate: opts?.gate ?? "M",
  qubits: [{ qubit }],
  results: [{ qubit, result: opts?.result ?? 0 }],
});

/**
 * Group (a unitary that owns a children grid).
 *
 *   group("Foo", [
 *     [gate("H", 0), gate("X", 1)],   // inner column 0
 *     [gate("Z", 1)],                 // inner column 1
 *   ])
 *   group("Foo", [...], { ctrls: [3] })  // quantum-controlled group
 *
 * The group's `.targets` is auto-derived as the union of every
 * direct child's `.targets` and `.controls` quantum wires
 * (recursively for nested groups, since each nested group's own
 * `.targets` is similarly auto-derived). The group's own controls
 * (from `opts.ctrls`) live on `.controls`, not `.targets` — same as
 * the real `CircuitModel`. Pass `opts.span` to force a wider extent
 * than the children imply (e.g. a group that visually covers a wire
 * none of its children touch).
 *
 * @param {string} name
 * @param {any[][]} innerGrid  array of inner columns, each column an
 *   array of ops (built with `gate` / `meas` / nested `group`)
 * @param {{ ctrls?: (number | { q: number, r?: number })[],
 *           conditional?: boolean,
 *           expanded?: boolean,
 *           span?: number[] }} [opts]
 * @returns {any}
 */
export const group = (name, innerGrid, opts) => {
  // Union of every direct child's quantum wire span (target qubits
  // + control qubits). Measurements expose `.qubits` instead of
  // `.targets`; controls always live on `.controls` regardless.
  const childWires = new Set();
  for (const col of innerGrid) {
    for (const child of col) {
      const targets = child.targets ?? [];
      const controls = child.controls ?? [];
      const measQubits = child.qubits ?? [];
      for (const t of targets) {
        if (typeof t.qubit === "number") childWires.add(t.qubit);
      }
      for (const c of controls) {
        if (typeof c.qubit === "number") childWires.add(c.qubit);
      }
      for (const q of measQubits) {
        if (typeof q.qubit === "number") childWires.add(q.qubit);
      }
    }
  }
  const wires = [...childWires].sort((a, b) => a - b);
  // `opts.span` overrides the derived extent when the group should
  // visually cover wires its children don't all occupy.
  const targetWires = opts?.span ?? wires;

  /** @type {any} */
  const out = {
    kind: "unitary",
    gate: name,
    targets: targetWires.map((q) => ({ qubit: q })),
    children: innerGrid.map((col) => ({ components: col })),
  };
  if (opts?.ctrls) {
    out.controls = opts.ctrls.map((c) =>
      typeof c === "number" ? { qubit: c } : { qubit: c.q, result: c.r ?? 0 },
    );
  }
  if (opts?.conditional) out.isConditional = true;
  // Mark the group as render-expanded (the renderer reads
  // `dataAttributes.expanded` to show the body instead of a
  // collapsed box).
  if (opts?.expanded) out.dataAttributes = { expanded: "true" };
  return out;
};

/**
 * Build a circuit literal.
 *
 *   circuit(4, [[gate("H", 0)], [gate("X", 1)]])
 *   circuit(qubits(4, { 0: 1 }), [...])   // qubits with numResults
 *
 * @param {number | any[]} numQubitsOrQubits
 * @param {any[][]} grid  outer grid: array of columns, each column an
 *   array of ops
 * @returns {any}
 */
export const circuit = (numQubitsOrQubits, grid) => ({
  qubits:
    typeof numQubitsOrQubits === "number"
      ? qubits(numQubitsOrQubits)
      : numQubitsOrQubits,
  componentGrid: grid.map((col) => ({ components: col })),
});

/**
 * Build a `CircuitModel` from a circuit literal.
 *
 * @param {any} circuitObj
 * @returns {any}
 */
export const build = (circuitObj) => new CircuitModel(circuitObj);

// ---------------------------------------------------------------
// Extraction
// ---------------------------------------------------------------

/**
 * Look up an op in `model` by location string. Cast to `any` for
 * ergonomic property access in tests.
 *
 * @param {any} model
 * @param {string} location  e.g. `"0,0"`, `"0,0-1,0"`, `"0,0-0,0-1,0"`
 * @returns {any}
 */
export const at = (model, location) =>
  /** @type {any} */ (findOperation(model.componentGrid, location));

/**
 * Wire indices of an op's targets.
 * @param {any} op
 * @returns {number[]}
 */
export const wires = (op) => op.targets.map((/** @type {any} */ t) => t.qubit);

/**
 * Wire indices of an op's controls (quantum or classical).
 * @param {any} op
 * @returns {number[]}
 */
export const ctrlWires = (op) =>
  (op.controls ?? []).map((/** @type {any} */ c) => c.qubit);

/**
 * Gate names of every top-level column, as a 2D array.
 *   [["Foo", "X"], ["Z"]]
 * @param {any} model
 * @returns {string[][]}
 */
export const topShape = (model) =>
  model.componentGrid.map((/** @type {any} */ col) =>
    col.components.map((/** @type {any} */ op) => op.gate),
  );

/**
 * Gate names of every inner column of a group, as a 2D array.
 * @param {any} groupOp
 * @returns {string[][]}
 */
export const innerShape = (groupOp) =>
  (groupOp.children ?? []).map((/** @type {any} */ col) =>
    col.components.map((/** @type {any} */ op) => op.gate),
  );

// ---------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------

/**
 * Assert the model's top-level grid matches the expected gate shape.
 * @param {any} model
 * @param {string[][]} expected
 */
export const assertTopShape = (model, expected) => {
  const actual = topShape(model);
  assert.deepEqual(
    actual,
    expected,
    `top-level grid shape mismatch; got ${JSON.stringify(actual)}, expected ${JSON.stringify(expected)}`,
  );
};

/**
 * Assert a group's inner grid matches the expected gate shape.
 * @param {any} groupOp
 * @param {string[][]} expected
 */
export const assertInnerShape = (groupOp, expected) => {
  const actual = innerShape(groupOp);
  assert.deepEqual(
    actual,
    expected,
    `inner grid shape mismatch; got ${JSON.stringify(actual)}, expected ${JSON.stringify(expected)}`,
  );
};

/**
 * Assert an op's target wires match `expected` (order-independent).
 * @param {any} op
 * @param {number[]} expected
 */
export const assertWires = (op, expected) => {
  const actual = [...wires(op)].sort((a, b) => a - b);
  const want = [...expected].sort((a, b) => a - b);
  assert.deepEqual(
    actual,
    want,
    `wire mismatch; got ${JSON.stringify(actual)}, expected ${JSON.stringify(want)}`,
  );
};

/**
 * Assert an op's target wires INCLUDE every wire in `required`.
 * @param {any} op
 * @param {...number} required
 */
export const assertEnclosesWires = (op, ...required) => {
  const w = wires(op);
  for (const q of required) {
    assert.ok(
      w.includes(q),
      `expected wires to include ${q}; got ${JSON.stringify(w)}`,
    );
  }
};

/**
 * Assert an op's target wires DO NOT include any wire in `excluded`.
 * @param {any} op
 * @param {...number} excluded
 */
export const assertExcludesWires = (op, ...excluded) => {
  const w = wires(op);
  for (const q of excluded) {
    assert.ok(
      !w.includes(q),
      `expected wires NOT to include ${q}; got ${JSON.stringify(w)}`,
    );
  }
};

// ---------------------------------------------------------------
// Shape-DSL assertions.
//
// `expectGrid(model, gridSpec)` and `expectOp(op, opSpec)` match
// against a declarative spec literal. Conventions:
//
//   - Objects (op spec bodies) are SUBSET matches: only declared
//     keys are checked.
//   - Arrays (`wires`, `qubits`, `ctrls`, `results`, `componentGrid`
//     columns, column components) are EXACT matches: length must
//     agree. `targets` / `qubits` / `ctrls` / `results` are sorted
//     before compare. Ops within a column / inner-column are
//     matched ORDER-INDEPENDENTLY (greedy): each spec item is
//     matched against any remaining actual op. Within a child
//     grid, COLUMN order IS load-bearing (columns are temporal).
//   - `children` is an ordered array of columns (temporal); each
//     column's ops are matched order-independently.
//
// Op spec grammar:
//   "H"                                 // gate name only, no further checks
//   { H: 2 }                            // sugar for { H: { targets: [2] } }
//   { H: [0, 2] }                       // sugar for { H: { targets: [0, 2] } }
//   { H: { targets, qubits, ctrls,      // full form; any subset of these
//          results, conditional,
//          children } }
//
// Leg shorthands inside `ctrls` and `results`:
//   3            ->  { qubit: 3 }                      (quantum control)
//   { q: 2 }     ->  { qubit: 2 }                      (quantum control)
//   { q: 2, r: 0 } -> { qubit: 2, result: 0 }          (classical leg)
// ---------------------------------------------------------------

/** @param {any} c */
const normCtrl = (c) =>
  typeof c === "number"
    ? { qubit: c }
    : c.r === undefined
      ? { qubit: c.q }
      : { qubit: c.q, result: c.r };

/** @param {any} c */
const actualCtrl = (c) =>
  c.result === undefined
    ? { qubit: c.qubit }
    : { qubit: c.qubit, result: c.result };

/** @param {any} r */
const normResult = (r) =>
  typeof r === "number"
    ? { qubit: r, result: 0 }
    : { qubit: r.q, result: r.r ?? 0 };

/** @param {any} r */
const actualResult = (r) => ({ qubit: r.qubit, result: r.result });

/**
 * @param {number[]} list
 * @returns {number[]}
 */
const sortNums = (list) => [...list].sort((a, b) => a - b);

/**
 * @param {{ qubit: number, result?: number }[]} list
 */
const sortLegs = (list) =>
  [...list].sort(
    (a, b) => a.qubit - b.qubit || (a.result ?? -1) - (b.result ?? -1),
  );

/**
 * @param {any} actual
 * @param {any} spec
 * @param {string} path
 */
const matchOp = (actual, spec, path) => {
  if (actual === undefined || actual === null) {
    assert.fail(`${path}: expected an op, got ${actual}`);
  }
  // Bare-string spec: only check gate name.
  if (typeof spec === "string") {
    assert.equal(actual.gate, spec, `${path}: gate name`);
    return;
  }
  const keys = Object.keys(spec);
  assert.equal(
    keys.length,
    1,
    `${path}: op spec must have exactly one key (the gate name); got ${JSON.stringify(keys)}`,
  );
  const gateName = keys[0];
  const body = spec[gateName];
  assert.equal(
    actual.gate,
    gateName,
    `${path}: expected gate "${gateName}", got "${actual.gate}"`,
  );

  // Wire shorthand: number or number[].
  const props =
    typeof body === "number"
      ? { targets: [body] }
      : Array.isArray(body)
        ? { targets: body }
        : body;

  const here = `${path}.${gateName}`;

  if (props.targets !== undefined) {
    const got = sortNums(
      (actual.targets ?? []).map((/** @type {any} */ t) => t.qubit),
    );
    const want = sortNums(props.targets);
    assert.deepEqual(got, want, `${here}.targets`);
  }
  if (props.qubits !== undefined) {
    const got = sortNums(
      (actual.qubits ?? []).map((/** @type {any} */ q) => q.qubit),
    );
    const want = sortNums(props.qubits);
    assert.deepEqual(got, want, `${here}.qubits`);
  }
  if (props.ctrls !== undefined) {
    const got = sortLegs((actual.controls ?? []).map(actualCtrl));
    const want = sortLegs(props.ctrls.map(normCtrl));
    assert.deepEqual(got, want, `${here}.ctrls`);
  }
  if (props.results !== undefined) {
    const got = sortLegs((actual.results ?? []).map(actualResult));
    const want = sortLegs(props.results.map(normResult));
    assert.deepEqual(got, want, `${here}.results`);
  }
  if (props.conditional !== undefined) {
    assert.equal(
      !!actual.isConditional,
      !!props.conditional,
      `${here}.conditional`,
    );
  }
  if (props.children !== undefined) {
    const childGrid = actual.children ?? [];
    assert.equal(
      childGrid.length,
      props.children.length,
      `${here}.children: column count (got ${childGrid.length}, expected ${props.children.length})`,
    );
    props.children.forEach((/** @type {any} */ col, /** @type {number} */ i) =>
      matchColumn(childGrid[i].components, col, `${here}.children[${i}]`),
    );
  }
};

/**
 * Try to match `actual` against `spec`. Returns true on success,
 * false on any mismatch. Used by `matchColumn` to greedy-pair
 * spec items with actual ops without depending on storage order.
 *
 * @param {any} actual
 * @param {any} spec
 */
const wouldMatchOp = (actual, spec) => {
  try {
    matchOp(actual, spec, "probe");
    return true;
  } catch {
    return false;
  }
};

/**
 * @param {any[]} actualOps
 * @param {any[]} specOps
 * @param {string} path
 */
const matchColumn = (actualOps, specOps, path) => {
  assert.equal(
    actualOps.length,
    specOps.length,
    `${path}: op count (got ${actualOps.length}, expected ${specOps.length})`,
  );
  // Order-independent (greedy): for each spec item, claim the
  // first unclaimed actual that matches. Column data order is
  // essentially insertion order and not load-bearing — the
  // renderer positions ops by wire, not by list index.
  const remaining = [...actualOps];
  specOps.forEach((spec, i) => {
    const idx = remaining.findIndex((op) => wouldMatchOp(op, spec));
    if (idx === -1) {
      // Re-run match against the first remaining op to surface a
      // useful diagnostic (gate-name mismatch, wires mismatch, etc).
      matchOp(remaining[0], spec, `${path}[${i}]`);
      assert.fail(
        `${path}[${i}]: no remaining op in column matches spec ${JSON.stringify(spec)}`,
      );
    }
    remaining.splice(idx, 1);
  });
};

/**
 * Assert that `op` matches the given spec (subset on object keys,
 * exact on arrays). See the comment block above for grammar.
 *
 * @param {any} op
 * @param {any} spec
 */
export const expectOp = (op, spec) => matchOp(op, spec, "op");

/**
 * Assert that `model`'s top-level grid matches the given spec
 * (a 2D array: outer = columns, inner = ops per column).
 *
 * @param {any} model
 * @param {any[][]} gridSpec
 */
export const expectGrid = (model, gridSpec) => {
  const grid = model.componentGrid;
  assert.equal(
    grid.length,
    gridSpec.length,
    `grid: column count (got ${grid.length}, expected ${gridSpec.length})`,
  );
  gridSpec.forEach((col, i) =>
    matchColumn(grid[i].components, col, `grid[${i}]`),
  );
};
