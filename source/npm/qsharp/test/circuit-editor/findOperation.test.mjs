// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// findOperation / findParentArray / findParentOperation tests.
//
// These are the location-walking helpers in `ux/circuit-vis/utils.ts`. They share a single private
// bounds-checked walker, so the tests focus on the contract every public helper must honor:
//
//   - null / empty location → null
//   - in-bounds location → the addressed thing
//   - out-of-bounds location (top-level or nested) → null, never throw
//
// "Out of bounds" matters in practice because event handlers read `data-location` attributes from
// the DOM, and those attributes can outlive the model state they were written against (re-render
// races, stale selection after an undo, hand-constructed locations, etc.).

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
 * Build a grid with one outer op (a "group") whose `children` is itself a 1-column grid containing
 * an inner X@0. The outer op lives at "0,0"; the inner op lives at "0,0-0,0".
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

// ---------- shared contract ----------

test("all three helpers return null for a null/empty location", () => {
  const grid = nestedGrid();
  for (const loc of [null, ""]) {
    assert.equal(findOperation(grid, loc), null);
    assert.equal(findParentArray(grid, loc), null);
    assert.equal(findParentOperation(grid, loc), null);
  }
});

test("findOperation returns null for out-of-bounds op indices", () => {
  const grid = nestedGrid();
  // Out-of-bounds top-level column / op, out-of-bounds nested column / op, and a path below a
  // missing ancestor.
  for (const loc of ["9,0", "0,9", "0,0-9,0", "0,0-0,9", "9,0-0,0"]) {
    assert.equal(findOperation(grid, loc), null, loc);
  }
});

test("findParentArray/findParentOperation return null when an ancestor is missing", () => {
  const grid = nestedGrid();
  // The parent PATH is valid, so the containing grid is returned even when the addressed op index
  // itself is out of bounds.
  assert.ok(findParentArray(grid, "9,0")); // root grid
  assert.ok(findParentArray(grid, "0,0-9,0")); // children grid
  // An ancestor in the path is missing → null.
  assert.equal(findParentArray(grid, "9,0-0,0"), null);
  assert.equal(findParentOperation(grid, "9,0-0,0"), null);
});

// ---------- findOperation ----------

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

// ---------- findParentArray ----------

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

// ---------- findParentOperation ----------

test("findParentOperation returns null at top level and the group at a nested location", () => {
  const grid = nestedGrid();
  // Top-level locations have no parent op.
  assert.equal(findParentOperation(grid, "0,0"), null);
  // Nested locations return the immediate parent op.
  const parent = findParentOperation(grid, "0,0-0,0");
  assert.ok(parent);
  assert.equal(parent.gate, "Group");
});
