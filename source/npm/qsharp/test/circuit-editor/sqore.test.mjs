// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// Sqore lifecycle tests — direct unit coverage for the bits of
// `sqore.ts` that aren't exercised through `renderCircuit` + JSDOM
// in the existing dropzone / integration suites. Today this is just
// `rebaseViewState`, the consumer side of the B11 ViewState-transfer
// pathway:
//
//   - The producer side (`moveOperation` stamping
//     `sqore-prev-location` onto the deep-cloned op) is covered by
//     [circuitActions.test.mjs](circuitActions.test.mjs) — three tests
//     verifying the stamp is written with the right pre-move location.
//   - The end-to-end "chevron click survives a move + re-render" path
//     is covered indirectly by an integration test in
//     [dropzones.test.mjs](dropzones.test.mjs).
//
// What was missing — and what this file pins — is direct coverage of
// the rebase method's three branches:
//
//   1. Identity preserved (op survived an in-place mutation, or moved
//      to a new column without being cloned): identity hit against the
//      live grid wins, entry is rekeyed to the new location.
//   2. Identity lost + `sqore-prev-location` stamp present (the moved-
//      via-deep-clone case): identity lookup misses, fallback by
//      pre-move location succeeds, entry rekeyed to the clone's
//      current location, AND the stamp is deleted from
//      `dataAttributes` so it never leaks into the rendered SVG or
//      re-triggers on the next rebase.
//   3. Identity lost + stamp absent (op vanished entirely — deleted,
//      or replaced by an unrelated edit): both lookups miss, entry
//      is dropped via `ViewState.rebase`'s null-newKey branch.
//
// Plus the trivial first-render no-op: when `lastLocationMap` is
// null (initial draw or post-`updateCircuit` snapshot invalidation),
// `rebaseViewState` short-circuits without touching `viewState`.
//
// Sqore's constructor is JSDOM-free (it doesn't touch the DOM until
// `draw()`), so these tests bypass the full render loop and drive
// `rebaseViewState` directly via the same `/** @type {any} */` cast
// pattern other controller tests use to reach private methods.

// @ts-check

import { afterEach, beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import { Sqore } from "../../dist/ux/circuit-vis/sqore.js";

// JSDOM-free — `Sqore`'s constructor only validates the circuit
// group; `rebaseViewState` is pure-data over `this.circuit`,
// `this.lastLocationMap`, and `this.viewState`.

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

test("rebaseViewState: no-op on the first render when lastLocationMap is null", () => {
  // The defining state of "we've never rendered before" — the
  // snapshot is null, and `rebaseViewState` must short-circuit
  // without touching `viewState`. Same state as immediately after
  // `updateCircuit` (which nulls the snapshot to drop the now-
  // unrelated identity link to the prior tree).
  const opA = {
    kind: "unitary",
    gate: "H",
    targets: [{ qubit: 0 }],
  };
  sqore = makeSqore([{ components: [opA] }]);
  // Default state: `lastLocationMap` is null. Pre-seed an entry
  // that should NOT be touched by the short-circuit branch.
  sqore.viewState.setExpanded("0,0", true);

  sqore.rebaseViewState();

  assert.equal(sqore.viewState.expanded.size, 1);
  assert.equal(sqore.viewState.expanded.get("0,0"), true);
});

test("rebaseViewState: identity-preserved op moved to a new column is rekeyed via the live identity lookup", () => {
  // Branch 1: the op survived in-place (no deep clone) but its
  // location string changed because an upstream edit shifted the
  // column. The op's object identity is preserved across the edit,
  // so `next.get(op)` hits and the entry is rekeyed from the prior
  // location to the new one. This is the common case for any edit
  // that doesn't go through `moveOperation`'s clone path — e.g. a
  // drag that inserted a new column ahead of the tracked op.
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
  // Branch 2: the B11 case. `moveOperation` did
  // `JSON.parse(JSON.stringify(...))` on the source op before
  // splicing it in, so the live grid holds a NEW object reference
  // even though it's logically the same op. The identity lookup
  // against `next` misses, but the clone carries
  // `dataAttributes["sqore-prev-location"] = <pre-move location>`,
  // which lets the fallback map recover the right post-move
  // location for the prior snapshot's entry.
  //
  // Critical bonus: the stamp MUST be deleted from
  // `dataAttributes` after consumption so it neither (a) leaks
  // into the rendered SVG as a stray `data-sqore-prev-location`
  // attribute nor (b) re-triggers on a subsequent rebase if the
  // op happens to be tracked through it.
  const oldOp = {
    kind: "unitary",
    gate: "H",
    targets: [{ qubit: 0 }],
  };
  // Distinct object reference — what the JSON.stringify/parse
  // pair would produce. Carries the stamp `moveOperation` writes.
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

  // Entry follows the stamp from "0,0" → "1,0" (the clone's new
  // location), even though oldOp's reference is nowhere in the
  // live grid.
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
  // Branch 3: an op tracked in the prior snapshot is no longer
  // anywhere in the live grid (e.g. deleted by a drag-out-delete,
  // or replaced by an unrelated edit), AND no replacement op
  // carries a `sqore-prev-location` pointing at the tracked op's
  // prior location. Both lookups miss; `remap.set(oldLoc, null)`
  // tells `ViewState.rebase` to drop the entry.
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
  // Defense-in-depth contract: a ViewState entry whose key is NOT
  // in the prior `lastLocationMap` at all (e.g. an entry the user
  // set via a programmatic API before the first tracked render)
  // must NOT be dropped just because the rebase has no information
  // about it. `ViewState.rebase` handles this via `!remap.has(oldKey)`;
  // we exercise it end-to-end here by setting an extra
  // `viewState` entry that doesn't correspond to any op in the
  // prior snapshot.
  const opA = {
    kind: "unitary",
    gate: "H",
    targets: [{ qubit: 0 }],
  };
  sqore = makeSqore([{ components: [opA] }]);
  sqore.lastLocationMap = snapshot([[opA, "0,0"]]);
  sqore.viewState.setExpanded("0,0", true);
  // Stray entry not represented in the snapshot. Should survive
  // unchanged.
  sqore.viewState.setExpanded("9,9", false);

  sqore.rebaseViewState();

  // Tracked entry stays at "0,0" (op didn't move); stray entry
  // stays at "9,9" untouched.
  assert.equal(sqore.viewState.expanded.size, 2);
  assert.equal(sqore.viewState.expanded.get("0,0"), true);
  assert.equal(sqore.viewState.expanded.get("9,9"), false);
});

test("rebaseViewState: handles nested ops — identity preserved at depth 2", () => {
  // The rebase walks the whole grid recursively (via
  // `buildLiveLocationMap`'s `walk` helper), so nested ops are
  // tracked too. Pin the nested-identity-preserved case: a child
  // of a group keeps its viewState entry across an edit that
  // shifted the group's column.
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
// updateCircuit — the React-wrapper escape hatch for external circuit
// updates. Direct unit coverage of the contract:
//
//   1. Replaces both `circuitGroup` and `circuit` (the first one in
//      the group) so subsequent renders use the new data.
//   2. Preserves `viewState` — the whole reason for the method's
//      existence. The React wrapper was throwing away every
//      user-expanded group on each external update before this
//      shipped.
//   3. NULLs `lastLocationMap` so the next rebase short-circuits as
//      a first-render (the new op identities have no relation to the
//      prior snapshot — running the rebase against the stale snapshot
//      would silently drop every entry by misidentifying ops via the
//      identity-lost branch).
//   4. Throws on null / empty input so a fumble in the host code
//      surfaces as an exception rather than a silently-broken render.
// ---------------------------------------------------------------------------

test("updateCircuit: swaps circuit + circuitGroup while preserving viewState", () => {
  // Pre-seed viewState the way a user would by expanding groups; the
  // central guarantee of `updateCircuit` is that these entries
  // survive the swap unchanged (vs. the pre-fix path of constructing
  // a brand-new Sqore, which destroyed them).
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
  // tree. Leaving `lastLocationMap` populated would cause every
  // tracked entry to fall through both identity and stamp lookups
  // and get dropped on the first post-`updateCircuit` rebase.
  // Nulling the snapshot is the explicit signal: treat the next
  // render as a fresh first render.
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

  // Now run the rebase: with the snapshot null, it must
  // short-circuit and leave viewState untouched (the
  // first-render contract from the no-op test above).
  sqore.rebaseViewState();
  assert.equal(sqore.viewState.expanded.size, 1);
  assert.equal(sqore.viewState.expanded.get("0,0"), true);
});

test("updateCircuit: throws on null circuitGroup", () => {
  sqore = makeSqore([
    { components: [{ kind: "unitary", gate: "H", targets: [{ qubit: 0 }] }] },
  ]);

  // Fumbled host code must surface as an exception, not a silently
  // broken render.
  assert.throws(() => sqore.updateCircuit(null), /No circuit found/);
});

test("updateCircuit: throws on circuitGroup with empty circuits array", () => {
  sqore = makeSqore([
    { components: [{ kind: "unitary", gate: "H", targets: [{ qubit: 0 }] }] },
  ]);

  // An empty `circuits` array is treated the same way as null — there
  // is nothing to render and continuing would NPE downstream when
  // `circuits[0]` was dereferenced.
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
  // Matches the constructor's behavior. `Sqore` today renders one
  // circuit at a time; if the host wants to switch to a later one in
  // the group, it would need a separate hook (none exists yet).
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
