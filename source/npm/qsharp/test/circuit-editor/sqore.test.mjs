// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// Sqore lifecycle tests — direct unit coverage of `rebaseViewState`
// and `updateCircuit`. These methods are JSDOM-free (they operate on
// `this.circuit`, `this.lastLocationMap`, and `this.viewState`), so
// tests drive them directly via `/** @type {any} */` casts to reach
// non-public members.

// @ts-check

import { afterEach, beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import { Sqore } from "../../dist/ux/circuit-vis/sqore.js";

/**
 * Build a fresh `Sqore` over a tiny single-circuit group containing
 * the given component grid. The grid is wrapped in a `CircuitGroup`
 * shaped exactly the way `qsharp-lang`'s `draw()` entrypoint expects.
 *
 * @param {any} componentGrid
 */
function makeSqore(componentGrid) {
  return new Sqore({
    circuits: [
      {
        qubits: [{ id: 0 }, { id: 1 }],
        componentGrid,
      },
    ],
  });
}

/**
 * Snapshot a `lastLocationMap`-shaped Map from a list of
 * `[op, location]` pairs. Mirrors what
 * `buildLiveLocationMap` produces at the end of a render.
 *
 * @param {Array<[any, string]>} pairs
 */
function snapshot(pairs) {
  return new Map(pairs);
}

/** @type {any} */
let sqore;

afterEach(() => {
  sqore = null;
});

// ---------------------------------------------------------------------------
// rebaseViewState: per-render key migration
// ---------------------------------------------------------------------------

test("rebaseViewState: no-op on the first render when lastLocationMap is null", () => {
  // No prior snapshot → short-circuit and leave `viewState` alone.
  const opA = {
    kind: "unitary",
    gate: "H",
    targets: [{ qubit: 0 }],
  };
  sqore = makeSqore([{ components: [opA] }]);
  // `lastLocationMap` defaults to null; pre-seed an entry the
  // short-circuit must not touch.
  sqore.viewState.setExpanded("0,0", true);

  sqore.rebaseViewState();

  assert.equal(sqore.viewState.expanded.size, 1);
  assert.equal(sqore.viewState.expanded.get("0,0"), true);
});

test("rebaseViewState: identity-preserved op moved to a new column is rekeyed via the live identity lookup", () => {
  // The op's object identity survives an upstream edit that shifts
  // its column. `next.get(op)` hits and the entry is rekeyed to the
  // new location.
  const opA = {
    kind: "unitary",
    gate: "H",
    targets: [{ qubit: 0 }],
  };
  const filler = {
    kind: "unitary",
    gate: "X",
    targets: [{ qubit: 0 }],
  };
  // Grid AFTER the edit: filler took column 0, opA shifted to column 1.
  sqore = makeSqore([{ components: [filler] }, { components: [opA] }]);
  // Snapshot from BEFORE the edit: opA was at "0,0".
  sqore.lastLocationMap = snapshot([[opA, "0,0"]]);
  sqore.viewState.setExpanded("0,0", true);

  sqore.rebaseViewState();

  // Entry rekeyed from "0,0" → "1,0"; original key is gone.
  assert.equal(sqore.viewState.expanded.size, 1);
  assert.equal(sqore.viewState.expanded.get("1,0"), true);
  assert.equal(sqore.viewState.expanded.has("0,0"), false);
});

test("rebaseViewState: identity-lost op with sqore-prev-location stamp is rekeyed AND the stamp is consumed", () => {
  // When `moveOperation` deep-clones an op, the live grid holds a
  // new object reference. The identity lookup misses; the clone's
  // `dataAttributes["sqore-prev-location"]` stamp recovers the
  // entry by pre-move location. The stamp must then be deleted so
  // it doesn't leak into the rendered SVG or re-trigger next rebase.
  const oldOp = {
    kind: "unitary",
    gate: "H",
    targets: [{ qubit: 0 }],
  };
  // Distinct object reference, carrying the stamp `moveOperation`
  // writes onto the clone.
  const clonedOp = {
    kind: "unitary",
    gate: "H",
    targets: [{ qubit: 1 }],
    dataAttributes: { "sqore-prev-location": "0,0" },
  };
  // Grid AFTER the move: clone landed at "1,0".
  sqore = makeSqore([
    { components: [{ kind: "unitary", gate: "X", targets: [{ qubit: 0 }] }] },
    { components: [clonedOp] },
  ]);
  // Snapshot from BEFORE the move: original oldOp was at "0,0".
  sqore.lastLocationMap = snapshot([[oldOp, "0,0"]]);
  sqore.viewState.setExpanded("0,0", true);

  sqore.rebaseViewState();

  // Entry follows the stamp from "0,0" → "1,0".
  assert.equal(sqore.viewState.expanded.size, 1);
  assert.equal(sqore.viewState.expanded.get("1,0"), true);
  assert.equal(sqore.viewState.expanded.has("0,0"), false);
  // Stamp consumed: must not survive to the next render.
  assert.equal(
    clonedOp.dataAttributes["sqore-prev-location"],
    undefined,
    "stamp must be deleted from dataAttributes after consumption",
  );
});

test("rebaseViewState: identity-lost op with no stamp drops the entry", () => {
  // A tracked op is gone from the live grid and no replacement
  // carries a stamp pointing at its prior location → drop the
  // entry.
  const goneOp = {
    kind: "unitary",
    gate: "H",
    targets: [{ qubit: 0 }],
  };
  // The replacement op has its own identity AND no stamp.
  const replacement = {
    kind: "unitary",
    gate: "X",
    targets: [{ qubit: 0 }],
  };
  sqore = makeSqore([{ components: [replacement] }]);
  sqore.lastLocationMap = snapshot([[goneOp, "0,0"]]);
  sqore.viewState.setExpanded("0,0", true);

  sqore.rebaseViewState();

  // No survivors — entry was dropped.
  assert.equal(sqore.viewState.expanded.size, 0);
});

test("rebaseViewState: untracked entries are left alone (ViewState.rebase no-touch contract)", () => {
  // A `viewState` entry whose key isn't in the snapshot must
  // survive — the rebase only mutates keys it has information
  // about.
  const opA = {
    kind: "unitary",
    gate: "H",
    targets: [{ qubit: 0 }],
  };
  sqore = makeSqore([{ components: [opA] }]);
  sqore.lastLocationMap = snapshot([[opA, "0,0"]]);
  sqore.viewState.setExpanded("0,0", true);
  // Stray entry not in the snapshot — must survive unchanged.
  sqore.viewState.setExpanded("9,9", false);

  sqore.rebaseViewState();

  // Tracked entry stays at "0,0" (op didn't move); stray entry
  // stays at "9,9" untouched.
  assert.equal(sqore.viewState.expanded.size, 2);
  assert.equal(sqore.viewState.expanded.get("0,0"), true);
  assert.equal(sqore.viewState.expanded.get("9,9"), false);
});

test("rebaseViewState: handles nested ops — identity preserved at depth 2", () => {
  // The rebase walks the grid recursively, so a group's child keeps
  // its viewState entry when an upstream edit shifts the group's
  // column.
  const childH = {
    kind: "unitary",
    gate: "H",
    targets: [{ qubit: 0 }],
  };
  const group = {
    kind: "unitary",
    gate: "Foo",
    targets: [{ qubit: 0 }, { qubit: 1 }],
    children: [{ components: [childH] }],
  };
  const filler = {
    kind: "unitary",
    gate: "X",
    targets: [{ qubit: 0 }],
  };
  // Grid AFTER the edit: filler took column 0, group + child
  // shifted to column 1.
  sqore = makeSqore([{ components: [filler] }, { components: [group] }]);
  // Snapshot from BEFORE the edit: group at "0,0", child at "0,0-0,0".
  sqore.lastLocationMap = snapshot([
    [group, "0,0"],
    [childH, "0,0-0,0"],
  ]);
  sqore.viewState.setExpanded("0,0", true);
  sqore.viewState.setExpanded("0,0-0,0", false);

  sqore.rebaseViewState();

  // Both entries rekeyed under the shifted prefix.
  assert.equal(sqore.viewState.expanded.size, 2);
  assert.equal(sqore.viewState.expanded.get("1,0"), true);
  assert.equal(sqore.viewState.expanded.get("1,0-0,0"), false);
  assert.equal(sqore.viewState.expanded.has("0,0"), false);
  assert.equal(sqore.viewState.expanded.has("0,0-0,0"), false);
});

// ---------------------------------------------------------------------------
// updateCircuit: the escape hatch for external circuit updates. Swaps
// `circuit` + `circuitGroup`, preserves `viewState`, and nulls
// `lastLocationMap` so the next rebase treats it as a first render.
// ---------------------------------------------------------------------------

test("updateCircuit: swaps circuit + circuitGroup while preserving viewState", () => {
  // Pre-seed viewState; the central guarantee is that these entries
  // survive the swap unchanged.
  sqore = makeSqore([
    { components: [{ kind: "unitary", gate: "H", targets: [{ qubit: 0 }] }] },
  ]);
  sqore.viewState.setExpanded("0,0", true);
  sqore.viewState.setExpanded("1,2-0,0", false);

  /** @type {any} */
  const newGroup = {
    circuits: [
      {
        qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
        componentGrid: [
          {
            components: [
              { kind: "unitary", gate: "X", targets: [{ qubit: 0 }] },
              { kind: "unitary", gate: "Y", targets: [{ qubit: 1 }] },
            ],
          },
        ],
      },
    ],
  };

  sqore.updateCircuit(newGroup);

  // circuitGroup swapped wholesale.
  assert.equal(sqore.circuitGroup, newGroup);
  // circuit is the FIRST circuit in the group (matches constructor).
  assert.equal(sqore.circuit, newGroup.circuits[0]);
  assert.equal(sqore.circuit.qubits.length, 3);

  // viewState entries survived intact — same keys, same values.
  assert.equal(sqore.viewState.expanded.size, 2);
  assert.equal(sqore.viewState.expanded.get("0,0"), true);
  assert.equal(sqore.viewState.expanded.get("1,2-0,0"), false);
});

test("updateCircuit: nullifies lastLocationMap so the next rebase short-circuits as first-render", () => {
  // The new circuit's op identities have no relation to the prior
  // snapshot. Nulling the map is the explicit signal to treat the
  // next render as a fresh first render.
  const opA = {
    kind: "unitary",
    gate: "H",
    targets: [{ qubit: 0 }],
  };
  sqore = makeSqore([{ components: [opA] }]);
  // Simulate a prior render having populated the location map.
  sqore.lastLocationMap = snapshot([[opA, "0,0"]]);
  sqore.viewState.setExpanded("0,0", true);

  /** @type {any} */
  const newGroup = {
    circuits: [
      {
        qubits: [{ id: 0 }],
        componentGrid: [
          {
            components: [
              { kind: "unitary", gate: "X", targets: [{ qubit: 0 }] },
            ],
          },
        ],
      },
    ],
  };

  sqore.updateCircuit(newGroup);

  assert.equal(sqore.lastLocationMap, null);

  // With the snapshot null, rebase must short-circuit and leave
  // viewState untouched.
  sqore.rebaseViewState();
  assert.equal(sqore.viewState.expanded.size, 1);
  assert.equal(sqore.viewState.expanded.get("0,0"), true);
});

test("updateCircuit: throws on null circuitGroup", () => {
  sqore = makeSqore([
    { components: [{ kind: "unitary", gate: "H", targets: [{ qubit: 0 }] }] },
  ]);

  // Host-side fumbles must surface as exceptions, not silent
  // broken renders.
  assert.throws(() => sqore.updateCircuit(null), /No circuit found/);
});

test("updateCircuit: throws on circuitGroup with empty circuits array", () => {
  sqore = makeSqore([
    { components: [{ kind: "unitary", gate: "H", targets: [{ qubit: 0 }] }] },
  ]);

  // Empty `circuits` is treated the same as null — nothing to
  // render.
  assert.throws(
    () => sqore.updateCircuit(/** @type {any} */ ({ circuits: [] })),
    /No circuit found/,
  );

  // Also null `circuits`.
  assert.throws(
    () =>
      sqore.updateCircuit(
        /** @type {any} */ ({ circuits: /** @type {any} */ (null) }),
      ),
    /No circuit found/,
  );
});

test("updateCircuit: when circuitGroup has multiple circuits, only the first becomes active", () => {
  // Matches the constructor: `Sqore` renders one circuit at a time.
  sqore = makeSqore([
    { components: [{ kind: "unitary", gate: "H", targets: [{ qubit: 0 }] }] },
  ]);

  /** @type {any} */
  const newGroup = {
    circuits: [
      {
        qubits: [{ id: 0 }],
        componentGrid: [
          {
            components: [
              { kind: "unitary", gate: "X", targets: [{ qubit: 0 }] },
            ],
          },
        ],
      },
      {
        qubits: [{ id: 0 }, { id: 1 }],
        componentGrid: [
          {
            components: [
              { kind: "unitary", gate: "Y", targets: [{ qubit: 0 }] },
            ],
          },
        ],
      },
    ],
  };

  sqore.updateCircuit(newGroup);

  assert.equal(sqore.circuitGroup, newGroup);
  assert.equal(sqore.circuit, newGroup.circuits[0]);
  // The first circuit had 1 qubit, not 2.
  assert.equal(sqore.circuit.qubits.length, 1);
});
