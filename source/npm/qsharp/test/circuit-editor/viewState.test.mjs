// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// Unit tests for `ViewState` — the per-session view-preference layer that survives
// `Sqore.renderCircuit()` but is not persisted to the `.qsc` file. See
// [viewState.ts](../../ux/circuit-vis/data/viewState.ts).
//
// These tests are pure data — no JSDOM. They lock down:
//   - Default state is empty.
//   - setExpanded(true) and setExpanded(false) both record overrides.
//   - Collapsing a parent prunes user overrides on its descendants so re-expanding doesn't
//     resurface stale child choices.
//   - applyTo writes overrides into a component grid, leaves non-overridden ops alone, and recurses
//     into children.

// @ts-check

import { test } from "node:test";
import assert from "node:assert/strict";
import { ViewState } from "../../dist/ux/circuit-vis/data/viewState.js";

// ---------------------------------------------------------------------------
// Storage primitives: setExpanded / clearExpanded
// ---------------------------------------------------------------------------

test("ViewState: setExpanded records expand and collapse choices", () => {
  const v = new ViewState();
  v.setExpanded("0,0", true);
  v.setExpanded("1,0", false);
  assert.equal(v.expanded.get("0,0"), true);
  assert.equal(v.expanded.get("1,0"), false);
  assert.equal(v.expanded.size, 2);
});

test("ViewState: setExpanded(true) does NOT clear descendants", () => {
  // Re-expanding a parent leaves descendant choices alone — the user's prior choices on the body of
  // the group resurface when the group is shown again.
  const v = new ViewState();
  v.setExpanded("0,0", true);
  v.setExpanded("0,0-1,0", false);
  v.setExpanded("0,0-2,1", true);

  v.setExpanded("0,0", true); // idempotent re-expand

  assert.equal(v.expanded.get("0,0-1,0"), false);
  assert.equal(v.expanded.get("0,0-2,1"), true);
});

test("ViewState: setExpanded(false) prunes descendant overrides", () => {
  // Collapsing a parent forgets descendant choices so re-expanding doesn't auto-spring
  // previously-expanded children back open.
  const v = new ViewState();
  v.setExpanded("0,0", true);
  v.setExpanded("0,0-1,0", true);
  v.setExpanded("0,0-2,1", false);
  v.setExpanded("1,0", true); // sibling, must NOT be pruned

  v.setExpanded("0,0", false);

  assert.equal(v.expanded.get("0,0"), false);
  assert.equal(
    v.expanded.has("0,0-1,0"),
    false,
    "descendant override should be pruned",
  );
  assert.equal(
    v.expanded.has("0,0-2,1"),
    false,
    "descendant override should be pruned",
  );
  assert.equal(
    v.expanded.get("1,0"),
    true,
    "sibling override must survive (not a descendant)",
  );
});

test("ViewState: setExpanded(false) does not match prefix of unrelated location", () => {
  // "0,10" is NOT a descendant of "0,1". The prune logic uses the `-` separator explicitly so
  // location-string substrings can't accidentally match.
  const v = new ViewState();
  v.setExpanded("0,1", true);
  v.setExpanded("0,10", true);

  v.setExpanded("0,1", false);

  assert.equal(v.expanded.get("0,10"), true, "0,10 is a sibling, not a child");
});

test("ViewState: clearExpanded drops the entry, falling back to defaults", () => {
  const v = new ViewState();
  v.setExpanded("0,0", true);
  v.clearExpanded("0,0");
  assert.equal(v.expanded.has("0,0"), false);
});

// ---------------------------------------------------------------------------
// applyTo: write overrides into a rendered component grid
// ---------------------------------------------------------------------------

test("ViewState: applyTo writes overrides into a component grid", () => {
  const v = new ViewState();
  v.setExpanded("0,0", true);
  v.setExpanded("1,0", false);

  /** @type {any} */
  const grid = [
    {
      components: [
        {
          kind: "unitary",
          gate: "Foo",
          targets: [{ qubit: 0 }],
          dataAttributes: { location: "0,0" },
          children: [],
        },
      ],
    },
    {
      components: [
        {
          kind: "unitary",
          gate: "Bar",
          targets: [{ qubit: 0 }],
          dataAttributes: { location: "1,0", expanded: "true" }, // pre-set, will be overridden
          children: [],
        },
      ],
    },
  ];

  v.applyTo(grid);

  assert.equal(grid[0].components[0].dataAttributes.expanded, "true");
  assert.equal(
    grid[1].components[0].dataAttributes.expanded,
    "false",
    "user collapse must override pre-existing default-expanded flag",
  );
});

test("ViewState: applyTo leaves non-overridden ops untouched", () => {
  const v = new ViewState();
  // No overrides at all.

  /** @type {any} */
  const grid = [
    {
      components: [
        {
          kind: "unitary",
          gate: "Foo",
          targets: [{ qubit: 0 }],
          dataAttributes: { location: "0,0", expanded: "true" }, // default-expanded
        },
        {
          kind: "unitary",
          gate: "Bar",
          targets: [{ qubit: 0 }],
          dataAttributes: { location: "0,1" }, // no flag
        },
      ],
    },
  ];

  v.applyTo(grid);

  assert.equal(
    grid[0].components[0].dataAttributes.expanded,
    "true",
    "no override → preserve existing flag",
  );
  assert.equal(
    grid[0].components[1].dataAttributes.expanded,
    undefined,
    "no override → preserve absence of flag",
  );
});

test("ViewState: applyTo recurses into children grids", () => {
  const v = new ViewState();
  v.setExpanded("0,0-1,0", true);

  /** @type {any} */
  const grid = [
    {
      components: [
        {
          kind: "unitary",
          gate: "Outer",
          targets: [{ qubit: 0 }],
          dataAttributes: { location: "0,0" },
          children: [
            { components: [] },
            {
              components: [
                {
                  kind: "unitary",
                  gate: "Inner",
                  targets: [{ qubit: 0 }],
                  dataAttributes: { location: "0,0-1,0" },
                  children: [],
                },
              ],
            },
          ],
        },
      ],
    },
  ];

  v.applyTo(grid);

  assert.equal(
    grid[0].components[0].children[1].components[0].dataAttributes.expanded,
    "true",
  );
});

test("ViewState: applyTo skips ops without a location attribute", () => {
  // Defensive: ops without a location can't be addressed by viewState entries anyway. applyTo must
  // not crash on them.
  const v = new ViewState();
  v.setExpanded("0,0", true);

  /** @type {any} */
  const grid = [
    {
      components: [
        {
          kind: "unitary",
          gate: "Foo",
          targets: [{ qubit: 0 }],
          // no dataAttributes
        },
        {
          kind: "unitary",
          gate: "Bar",
          targets: [{ qubit: 0 }],
          dataAttributes: { location: "0,0" },
        },
      ],
    },
  ];

  v.applyTo(grid);

  // Op without location: untouched.
  assert.equal(grid[0].components[0].dataAttributes, undefined);
  // Op with location and matching override: flag set.
  assert.equal(grid[0].components[1].dataAttributes.expanded, "true");
});

// ---------------------------------------------------------------------------
// rebase: key-migration across editor mutations.
//
// `Sqore` snapshots an op → location map after every render and calls `rebase` at the start of the
// next render with the (oldLoc → newLoc | null) derived from object identity. These tests pin down
// the pure-data rewrite semantics that `Sqore.rebaseViewState()` relies on.
// ---------------------------------------------------------------------------

test("ViewState: rebase rekeys entries to their new locations", () => {
  // User expanded "0,1"; the op then shifted to "0,2" because a sibling was inserted ahead of it.
  // The expanded state must follow the op.
  const v = new ViewState();
  v.setExpanded("0,1", true);

  v.rebase(new Map([["0,1", "0,2"]]));

  assert.equal(v.expanded.has("0,1"), false, "old key removed");
  assert.equal(v.expanded.get("0,2"), true, "new key carries the value");
  assert.equal(v.expanded.size, 1);
});

test("ViewState: rebase drops entries when the op is gone", () => {
  // User expanded "1,0"; the op was then deleted (drag-out-delete). `null` in the remap signals "op
  // no longer in the grid" and the entry must be dropped.
  const v = new ViewState();
  v.setExpanded("1,0", true);
  v.setExpanded("0,0", false);

  v.rebase(
    new Map([
      ["1,0", null],
      ["0,0", "0,0"], // unchanged
    ]),
  );

  assert.equal(v.expanded.has("1,0"), false, "removed op's entry dropped");
  assert.equal(v.expanded.get("0,0"), false, "unchanged entry preserved");
});

test("ViewState: rebase leaves untracked keys untouched", () => {
  // If the caller has no information about an old key (key absent from the remap), the entry must
  // stay. This is the "safe default" path — Sqore exercises it on the very first render (no prior
  // snapshot) and the rebase becomes a no-op.
  const v = new ViewState();
  v.setExpanded("0,0", true);
  v.setExpanded("1,0", false);

  v.rebase(new Map()); // empty remap

  assert.equal(v.expanded.get("0,0"), true);
  assert.equal(v.expanded.get("1,0"), false);
  assert.equal(v.expanded.size, 2);
});

test("ViewState: rebase preserves the recorded value (true vs false)", () => {
  // Migration must carry the user's collapse choice forward just as it carries an expand choice.
  // Tests both polarities in one shot.
  const v = new ViewState();
  v.setExpanded("0,0", true);
  v.setExpanded("0,1", false);

  v.rebase(
    new Map([
      ["0,0", "0,1"],
      ["0,1", "0,2"],
    ]),
  );

  assert.equal(v.expanded.get("0,1"), true, "expand carried forward");
  assert.equal(v.expanded.get("0,2"), false, "collapse carried forward");
});

test("ViewState: rebase is a no-op when newKey === oldKey", () => {
  // Identity remap entries should not perturb the underlying map — they happen in droves on renders
  // that don't shift any ops.
  const v = new ViewState();
  v.setExpanded("0,0", true);

  v.rebase(new Map([["0,0", "0,0"]]));

  assert.equal(v.expanded.get("0,0"), true);
  assert.equal(v.expanded.size, 1);
});

test("ViewState: rebase handles a key swap correctly", () => {
  // Two ops swap positions. The remap names both. Each entry should end up at the other's old
  // location. The order matters internally (snapshot pairs before mutating) but the visible result
  // must be a clean swap regardless.
  const v = new ViewState();
  v.setExpanded("0,0", true);
  v.setExpanded("0,1", false);

  v.rebase(
    new Map([
      ["0,0", "0,1"],
      ["0,1", "0,0"],
    ]),
  );

  assert.equal(v.expanded.get("0,0"), false, "0,1's value moved to 0,0");
  assert.equal(v.expanded.get("0,1"), true, "0,0's value moved to 0,1");
  assert.equal(v.expanded.size, 2);
});

test("ViewState: rebase rekeys nested locations the same way", () => {
  // Descendant locations inside an expanded group follow the same rekey path — the algorithm is
  // purely string-based.
  const v = new ViewState();
  v.setExpanded("0,1-1,0", true); // an op inside the group at 0,1

  // Group itself shifted from 0,1 to 0,2 (sibling inserted), so the descendant also moves.
  v.rebase(new Map([["0,1-1,0", "0,2-1,0"]]));

  assert.equal(v.expanded.has("0,1-1,0"), false);
  assert.equal(v.expanded.get("0,2-1,0"), true);
});
