// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// findOperation / findParentArray / findParentOperation tests.
//
// These are the location-walking helpers in `ux/circuit-vis/utils.ts`.
// They share a single private bounds-checked walker, so the tests focus
// on the contract every public helper must honor:
//
//   - null / empty location → null
//   - in-bounds location → the addressed thing
//   - out-of-bounds location (top-level or nested) → null, never throw
//
// "Out of bounds" matters in practice because event handlers read
// `data-location` attributes from the DOM, and those attributes can
// outlive the model state they were written against (re-render races,
// stale selection after an undo, hand-constructed locations, etc.).

// @ts-check

import { test } from "node:test";
import assert from "node:assert/strict";
import {
  findOperation,
  findParentArray,
  findParentOperation,
} from "../../dist/ux/circuit-vis/utils.js";

/** @typedef {import("../../dist/ux/circuit-vis/data/circuit.js").ComponentGrid} ComponentGrid */

/**
 * Build a 1-column grid with a single H@0.
 * @returns {ComponentGrid}
 */
function flatGrid() {
  return [
    {
      components: [{ kind: "unitary", gate: "H", targets: [{ qubit: 0 }] }],
    },
  ];
}

/**
 * Build a grid with one outer op (a "group") whose `children` is itself a
 * 1-column grid containing an inner X@0. The outer op lives at "0,0";
 * the inner op lives at "0,0-0,0".
 * @returns {ComponentGrid}
 */
function nestedGrid() {
  return [
    {
      components: [
        {
          kind: "unitary",
          gate: "Group",
          targets: [{ qubit: 0 }],
          children: [
            {
              components: [
                { kind: "unitary", gate: "X", targets: [{ qubit: 0 }] },
              ],
            },
          ],
        },
      ],
    },
  ];
}

// ---------- findOperation ----------

test("findOperation returns null for null/empty location", () => {
  const grid = flatGrid();
  assert.equal(findOperation(grid, null), null);
  assert.equal(findOperation(grid, ""), null);
});

test("findOperation returns the op at an in-bounds top-level location", () => {
  const grid = flatGrid();
  const op = findOperation(grid, "0,0");
  assert.ok(op);
  assert.equal(op.gate, "H");
});

test("findOperation returns the op at an in-bounds nested location", () => {
  const grid = nestedGrid();
  const op = findOperation(grid, "0,0-0,0");
  assert.ok(op);
  assert.equal(op.gate, "X");
});

test("findOperation returns null for an out-of-bounds top-level column", () => {
  const grid = flatGrid();
  assert.equal(findOperation(grid, "9,0"), null);
});

test("findOperation returns null for an out-of-bounds top-level op", () => {
  const grid = flatGrid();
  assert.equal(findOperation(grid, "0,9"), null);
});

test("findOperation returns null for an out-of-bounds nested location", () => {
  const grid = nestedGrid();
  // Outer op exists at "0,0", but its children grid only has "0,0".
  assert.equal(findOperation(grid, "0,0-9,0"), null);
  assert.equal(findOperation(grid, "0,0-0,9"), null);
});

test("findOperation returns null when the parent path is itself out of bounds", () => {
  const grid = nestedGrid();
  // No op at "9,0", so the nested address can't resolve.
  assert.equal(findOperation(grid, "9,0-0,0"), null);
});

// ---------- findParentArray ----------

test("findParentArray returns null for null/empty location", () => {
  const grid = flatGrid();
  assert.equal(findParentArray(grid, null), null);
  assert.equal(findParentArray(grid, ""), null);
});

test("findParentArray returns the root grid for a top-level location", () => {
  const grid = flatGrid();
  const parent = findParentArray(grid, "0,0");
  assert.equal(parent, grid);
});

test("findParentArray returns the children grid for a nested location", () => {
  const grid = nestedGrid();
  const parent = findParentArray(grid, "0,0-0,0");
  assert.ok(parent);
  // The children grid has exactly one column with the X gate.
  assert.equal(parent.length, 1);
  assert.equal(parent[0].components[0].gate, "X");
});

test("findParentArray returns null when an ancestor address is out of bounds", () => {
  const grid = nestedGrid();
  // Walking "9,0-..." can't get past the first segment.
  assert.equal(findParentArray(grid, "9,0-0,0"), null);
});

// ---------- findParentOperation ----------

test("findParentOperation returns null for top-level locations (no parent op)", () => {
  const grid = nestedGrid();
  assert.equal(findParentOperation(grid, "0,0"), null);
});

test("findParentOperation returns null for null/empty location", () => {
  const grid = flatGrid();
  assert.equal(findParentOperation(grid, null), null);
  assert.equal(findParentOperation(grid, ""), null);
});

test("findParentOperation returns the immediate parent for a nested location", () => {
  const grid = nestedGrid();
  const parent = findParentOperation(grid, "0,0-0,0");
  assert.ok(parent);
  assert.equal(parent.gate, "Group");
});

test("findParentOperation returns null when the parent address is out of bounds", () => {
  const grid = nestedGrid();
  // "9,0-0,0" → parent address is "9,0", which doesn't exist.
  assert.equal(findParentOperation(grid, "9,0-0,0"), null);
});
