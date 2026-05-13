// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// Unit tests for `ViewState` — the per-session view-preference layer
// that survives `Sqore.renderCircuit()` but is not persisted to the
// `.qsc` file. See [viewState.ts](../../ux/circuit-vis/data/viewState.ts).
//
// These tests are pure data — no JSDOM. They lock down:
//   - Default state is empty.
//   - setExpanded(true) and setExpanded(false) both record overrides.
//   - Collapsing a parent prunes user overrides on its descendants
//     (matches the original `collapseOperation` semantics).
//   - applyTo writes overrides into a component grid, leaves
//     non-overridden ops alone, and recurses into children.

// @ts-check

import { test } from "node:test";
import assert from "node:assert/strict";
import { ViewState } from "../../dist/ux/circuit-vis/data/viewState.js";

test("ViewState: starts empty", () => {
  const v = new ViewState();
  assert.equal(v.expanded.size, 0);
});

test("ViewState: setExpanded records expand and collapse choices", () => {
  const v = new ViewState();
  v.setExpanded("0,0", true);
  v.setExpanded("1,0", false);
  assert.equal(v.expanded.get("0,0"), true);
  assert.equal(v.expanded.get("1,0"), false);
  assert.equal(v.expanded.size, 2);
});

test("ViewState: setExpanded(true) does NOT clear descendants", () => {
  // Re-expanding a parent should leave previously-recorded descendant
  // choices alone — the user's prior choices on the body of the group
  // should resurface when the group is shown again.
  const v = new ViewState();
  v.setExpanded("0,0", true);
  v.setExpanded("0,0-1,0", false);
  v.setExpanded("0,0-2,1", true);

  v.setExpanded("0,0", true); // idempotent re-expand

  assert.equal(v.expanded.get("0,0-1,0"), false);
  assert.equal(v.expanded.get("0,0-2,1"), true);
});

test("ViewState: setExpanded(false) prunes descendant overrides", () => {
  // Mirrors the original `collapseOperation` behavior: collapsing
  // a parent forgets descendant choices so re-expanding doesn't
  // auto-spring previously-expanded children back open.
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
  // "0,1" is NOT a descendant of "0,1-extra"; "0,10" is NOT a
  // descendant of "0,1". The prune logic uses the `-` separator
  // explicitly so location-string substrings can't accidentally match.
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

test("ViewState: clearExpanded is idempotent on absent entries", () => {
  const v = new ViewState();
  v.clearExpanded("nonexistent");
  assert.equal(v.expanded.size, 0);
});

test("ViewState: applyTo writes overrides into a component grid", () => {
  const v = new ViewState();
  v.setExpanded("0,0", true);
  v.setExpanded("1,0", false);

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
  // Defensive: ops without a location can't be addressed by viewState
  // entries anyway. applyTo must not crash on them.
  const v = new ViewState();
  v.setExpanded("0,0", true);

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
