// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// Dropzone-layer tests: locks down the location strings emitted by
// the circuit editor's drop-target generator. Covers the drag/drop
// surface so positioning regressions don't sneak through.
//
// Tests render a small circuit through `draw()` with editor enabled,
// then inspect the resulting `g.dropzone-layer` for the set of
// `data-dropzone-location` attributes produced. We assert on location
// strings only — pixel positioning is a visual concern not covered
// here.

// @ts-check

import { JSDOM } from "jsdom";
import { afterEach, beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import { draw } from "../dist/ux/circuit-vis/index.js";

const documentTemplate = `<!doctype html><html>
  <head></head>
  <body></body>
</html>`;

/** @type {JSDOM | null} */
let jsdom = null;

beforeEach(() => {
  jsdom = new JSDOM(documentTemplate);
  // @ts-expect-error - the `jsdom` typings and DOM typings don't match
  globalThis.window = jsdom.window;
  globalThis.document = jsdom.window.document;
  globalThis.Node = jsdom.window.Node;
  globalThis.HTMLElement = jsdom.window.HTMLElement;
  globalThis.SVGElement = jsdom.window.SVGElement;
  globalThis.XMLSerializer = jsdom.window.XMLSerializer;
});

afterEach(() => {
  jsdom?.window.close();
  jsdom = null;
});

/**
 * Render a CircuitGroup with the editor enabled (so `createDropzones`
 * runs) and return the dropzone descriptors found in the resulting SVG.
 *
 * @param {import("../dist/ux/circuit-vis/index.js").CircuitGroup} group
 * @returns {{ location: string; wire: number; interColumn: boolean }[]}
 */
function renderAndCollectDropzones(group) {
  const container = document.createElement("div");
  container.className = "qs-circuit";
  document.body.appendChild(container);

  draw(group, container, {
    editor: {
      // No-op editCallback — we just need the editor branch to run so
      // dropzones are created.
      editCallback: () => {},
    },
    // Ask for a deep render so any expanded groups in the input are
    // actually rendered as expanded (not auto-collapsed).
    renderDepth: 5,
  });

  const dropzones = container.querySelectorAll(
    "g.dropzone-layer rect.dropzone[data-dropzone-location]",
  );
  return Array.from(dropzones).map((rect) => ({
    location: rect.getAttribute("data-dropzone-location") ?? "",
    wire: Number(rect.getAttribute("data-dropzone-wire") ?? "-1"),
    interColumn: rect.getAttribute("data-dropzone-inter-column") === "true",
  }));
}

/**
 * Build a minimal CircuitGroup wrapping a single Circuit. Keeps the
 * test fixtures readable by hiding the boilerplate.
 *
 * @param {{ qubits: import("../dist/ux/circuit-vis/index.js").Qubit[];
 *           componentGrid: import("../dist/ux/circuit-vis/index.js").ComponentGrid; }} circuit
 * @returns {import("../dist/ux/circuit-vis/index.js").CircuitGroup}
 */
function singleCircuit(circuit) {
  return {
    circuits: [circuit],
  };
}

// ---------------------------------------------------------------------------
// Regression baseline — circuits without any expanded groups should
// continue to produce only top-level dropzones (single-segment locations
// like "0,0"). This test passes today and is the load-bearing guard
// against Phase A accidentally changing top-level behavior.
// ---------------------------------------------------------------------------

test("flat circuit emits only top-level dropzones", () => {
  // Two qubits, two columns: H on q0, then CNOT (control q0, target q1).
  // No groups; nothing to recurse into.
  const group = singleCircuit({
    qubits: [{ id: 0 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "H",
            targets: [{ qubit: 0 }],
          },
        ],
      },
      {
        components: [
          {
            kind: "unitary",
            gate: "X",
            targets: [{ qubit: 1 }],
            controls: [{ qubit: 0 }],
          },
        ],
      },
    ],
  });

  const dropzones = renderAndCollectDropzones(group);

  // Every emitted dropzone should have a single-segment location (no `-`).
  const nested = dropzones.filter((d) => d.location.includes("-"));
  assert.deepEqual(
    nested,
    [],
    `flat circuit should not emit nested-location dropzones, got: ${JSON.stringify(nested)}`,
  );

  // Sanity check: at least some top-level dropzones were produced.
  assert.ok(
    dropzones.length > 0,
    "expected at least some dropzones to be produced for a non-empty circuit",
  );
});

// ---------------------------------------------------------------------------
// Phase A target: an expanded custom-gate group should produce
// dropzones inside its body. The location strings of those dropzones
// must be nested (start with the parent's location, followed by `-`).
// ---------------------------------------------------------------------------

test("expanded group emits nested-location dropzones inside its body", () => {
  // A custom gate `Foo` containing one nested `H`. We mark `Foo` as
  // explicitly expanded via `dataAttributes` so the renderer shows its
  // body (which is what the editor does when the user clicks the
  // expand chevron).
  const group = singleCircuit({
    qubits: [{ id: 0 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Foo",
            targets: [{ qubit: 0 }, { qubit: 1 }],
            dataAttributes: { expanded: "true" },
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
  });

  const dropzones = renderAndCollectDropzones(group);

  // We expect at least one nested dropzone — one with a location string
  // that starts with the parent's "0,0" prefix (the only top-level op).
  const nested = dropzones.filter((d) => d.location.startsWith("0,0-"));
  assert.ok(
    nested.length > 0,
    `expected nested dropzones inside expanded Foo group, got locations: ${JSON.stringify(
      dropzones.map((d) => d.location),
    )}`,
  );
});

// ---------------------------------------------------------------------------
// Phase A wire-extent clipping: an expanded group spanning only a
// subset of wires must not emit nested dropzones on wires outside its
// extent. Without this clipping, the editor would let a user "drop
// into" Foo on wire 2 even though Foo only spans wires 0 and 1, which
// the data model can't represent without silently extending Foo's
// targets.
// ---------------------------------------------------------------------------

test("nested dropzones are clipped to the group's wire extent", () => {
  // 3 qubits. Foo only spans wires 0-1; wire 2 has its own X gate
  // sitting alongside (so the renderer keeps wire 2 visible).
  const group = singleCircuit({
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Foo",
            targets: [{ qubit: 0 }, { qubit: 1 }],
            dataAttributes: { expanded: "true" },
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
          {
            kind: "unitary",
            gate: "X",
            targets: [{ qubit: 2 }],
          },
        ],
      },
    ],
  });

  const dropzones = renderAndCollectDropzones(group);
  const nested = dropzones.filter((d) => d.location.startsWith("0,0-"));

  // First: nested dropzones must actually exist — otherwise the
  // clipping assertion below is vacuously true and would silently
  // hide a Phase A regression where recursion stops emitting them.
  assert.ok(
    nested.length > 0,
    "expected some nested dropzones inside expanded Foo group",
  );

  // Then: none of them may target wire 2, which lies outside Foo's
  // [0, 1] extent.
  const leaked = nested.filter((d) => d.wire >= 2);
  assert.deepEqual(
    leaked,
    [],
    `nested dropzones must be clipped to Foo's wire extent (wires 0-1); leaked: ${JSON.stringify(
      leaked,
    )}`,
  );
});

// ---------------------------------------------------------------------------
// Pixel-coordinate tests. These tests assert that for every rendered
// gate, the on-column dropzone with the matching
// `data-dropzone-location` covers the gate's x range. If they pass,
// dropping a gate on top of an existing gate will land on a real
// dropzone — which is the thing the user actually relies on.
// ---------------------------------------------------------------------------

/**
 * Render a CircuitGroup with the editor enabled and return both the
 * rendered host elements (the gate boxes) and the produced dropzones,
 * each annotated with bounding-box coordinates pulled from SVG attrs.
 *
 * @param {import("../dist/ux/circuit-vis/index.js").CircuitGroup} group
 */
function renderAndCollectGeometry(group) {
  const container = document.createElement("div");
  container.className = "qs-circuit";
  document.body.appendChild(container);

  draw(group, container, {
    editor: { editCallback: () => {} },
    renderDepth: 5,
  });

  // The gate body box carries `data-width` and `x` in absolute SVG
  // coords. `data-location` lives on a parent `<g class="gate">` (set
  // via dataAttributes spread). So: find every box, walk up to the
  // closest `[data-location]`. Skip control circles / swap markers
  // (those are sibling elements, not the canonical body).
  const boxSelector = "[data-width][x]";
  const hosts = Array.from(container.querySelectorAll(boxSelector))
    .filter(
      (el) =>
        !el.classList.contains("gate-control") &&
        !el.classList.contains("gate-swap"),
    )
    .map((el) => {
      const gateGroup = el.closest("[data-location]");
      return {
        location: gateGroup?.getAttribute("data-location") ?? null,
        x: Number(el.getAttribute("x") ?? "NaN"),
        width: Number(el.getAttribute("data-width") ?? "NaN"),
      };
    })
    .filter((h) => h.location != null);

  // Dropzones — every on-column rect (interColumn=false) carries a
  // `data-dropzone-location` whose value matches a host's location.
  const dzSelector =
    "g.dropzone-layer rect.dropzone[data-dropzone-location][data-dropzone-inter-column='false']";
  const dropzones = Array.from(container.querySelectorAll(dzSelector)).map(
    (el) => ({
      location: el.getAttribute("data-dropzone-location") ?? "",
      x: Number(el.getAttribute("x") ?? "NaN"),
      width: Number(el.getAttribute("width") ?? "NaN"),
      wire: Number(el.getAttribute("data-dropzone-wire") ?? "-1"),
    }),
  );

  return { hosts, dropzones };
}

/**
 * Parse a hierarchical location string into its scope prefix and the
 * (colIndex, opIndex) inside that scope.
 *
 *   "0,0"     -> { prefix: "",     colIndex: 0, opIndex: 0 }
 *   "0,0-1,2" -> { prefix: "0,0",  colIndex: 1, opIndex: 2 }
 */
function parseLocation(loc) {
  const lastDash = loc.lastIndexOf("-");
  const prefix = lastDash === -1 ? "" : loc.slice(0, lastDash);
  const tail = lastDash === -1 ? loc : loc.slice(lastDash + 1);
  const [colIndex, opIndex] = tail.split(",").map(Number);
  return { prefix, colIndex, opIndex };
}

/**
 * For each rendered gate `host`, assert there is at least one on-column
 * dropzone in the *same column* (same prefix + colIndex) whose x-range
 * overlaps the host's x-range. This is the geometry property that
 * actually matters for the editor: dropping near a gate must hit a
 * dropzone in that gate's column.
 *
 * Note: we deliberately match on (prefix, colIndex) — NOT full location
 * — because a column with N ops emits dropzones at opIndex `0..N`
 * (slots above each op + the trailing slot). Every dropzone in that
 * column has the same x/width as every other (they're all sized to
 * the column), so any one of them is a valid coverage witness.
 */
function assertHostsCoveredByColumnDropzones(hosts, dropzones, label) {
  for (const host of hosts) {
    const hostKey = parseLocation(host.location);
    const sameCol = dropzones.filter((d) => {
      const dzKey = parseLocation(d.location);
      return (
        dzKey.prefix === hostKey.prefix && dzKey.colIndex === hostKey.colIndex
      );
    });
    assert.ok(
      sameCol.length > 0,
      `${label}: no on-column dropzone in column "${hostKey.prefix}":${hostKey.colIndex} for host ${host.location}`,
    );

    const hostLeft = host.x;
    const hostRight = host.x + host.width;
    const dzWitness = sameCol[0]; // all share the same x/width
    const dzLeft = dzWitness.x;
    const dzRight = dzWitness.x + dzWitness.width;
    assert.ok(
      dzRight >= hostLeft && dzLeft <= hostRight,
      `${label}: column "${hostKey.prefix}":${hostKey.colIndex} dropzone (x=[${dzLeft}, ${dzRight}]) does not overlap host ${host.location} (x=[${hostLeft}, ${hostRight}])`,
    );
  }
}

test("flat circuit: every gate is covered by its on-column dropzone", () => {
  // Two 1-qubit gates and one CNOT — three columns, all at top level.
  const group = singleCircuit({
    qubits: [{ id: 0 }, { id: 1 }],
    componentGrid: [
      {
        components: [{ kind: "unitary", gate: "H", targets: [{ qubit: 0 }] }],
      },
      {
        components: [{ kind: "unitary", gate: "T", targets: [{ qubit: 1 }] }],
      },
      {
        components: [
          {
            kind: "unitary",
            gate: "X",
            targets: [{ qubit: 1 }],
            controls: [{ qubit: 0 }],
          },
        ],
      },
    ],
  });

  const { hosts, dropzones } = renderAndCollectGeometry(group);

  assert.ok(hosts.length > 0, "expected at least some host elements");
  assertHostsCoveredByColumnDropzones(hosts, dropzones, "flat circuit");
});

test("expanded group: nested gates are covered by their on-column dropzones", () => {
  // The Phase A bug: nested dropzones existed (correct location strings)
  // but landed at the wrong x positions, so users couldn't hit them.
  // This test would have caught that regression.
  const group = singleCircuit({
    qubits: [{ id: 0 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Foo",
            targets: [{ qubit: 0 }, { qubit: 1 }],
            dataAttributes: { expanded: "true" },
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
              {
                components: [
                  {
                    kind: "unitary",
                    gate: "X",
                    targets: [{ qubit: 1 }],
                  },
                ],
              },
            ],
          },
        ],
      },
    ],
  });

  const { hosts, dropzones } = renderAndCollectGeometry(group);

  // Filter to nested hosts — the gates inside Foo's body. Their
  // location strings start with "0,0-".
  const nestedHosts = hosts.filter((h) => h.location.startsWith("0,0-"));
  assert.ok(
    nestedHosts.length > 0,
    `expected some nested host elements inside Foo, got hosts: ${JSON.stringify(
      hosts.map((h) => h.location),
    )}`,
  );

  assertHostsCoveredByColumnDropzones(nestedHosts, dropzones, "expanded group");
});

// ---------------------------------------------------------------------------
// Editor overlay structure. All editor-only DOM (dropzones, ghost
// qubit row, future overlays) must live inside a single
// `g.editor-overlay` group attached to `svg.qviz`. The renderer
// (formatGates / formatRegisters / formatInputs) never touches that
// group; the editor never appends outside it. Asserting the
// containment property keeps future contributors honest about which
// side of the boundary their DOM belongs on.
// ---------------------------------------------------------------------------

test("editor-only DOM lives inside the editor-overlay group", () => {
  const group = singleCircuit({
    qubits: [{ id: 0 }, { id: 1 }],
    componentGrid: [
      {
        components: [{ kind: "unitary", gate: "H", targets: [{ qubit: 0 }] }],
      },
      {
        components: [
          {
            kind: "unitary",
            gate: "X",
            targets: [{ qubit: 1 }],
            controls: [{ qubit: 0 }],
          },
        ],
      },
    ],
  });

  const container = document.createElement("div");
  container.className = "qs-circuit";
  document.body.appendChild(container);
  draw(group, container, { editor: { editCallback: () => {} } });

  const svg = container.querySelector("svg.qviz");
  assert.ok(svg, "expected an svg.qviz");

  // Exactly one editor-overlay group, attached as a direct child of
  // svg.qviz.
  const overlays = svg.querySelectorAll("g.editor-overlay");
  assert.equal(overlays.length, 1, "expected exactly one editor-overlay");
  assert.equal(
    overlays[0].parentElement,
    svg,
    "editor-overlay must be a direct child of svg.qviz",
  );

  // Both editor-only sub-layers must live inside the overlay.
  const overlay = overlays[0];
  assert.equal(
    overlay.querySelectorAll("g.dropzone-layer").length,
    1,
    "dropzone-layer must live inside editor-overlay",
  );
  assert.equal(
    overlay.querySelectorAll("g.ghost-qubit-layer").length,
    1,
    "ghost-qubit-layer must live inside editor-overlay",
  );

  // No editor-only layers may exist as direct children of svg.qviz
  // outside the overlay. Walk svg's direct children and verify.
  const directChildLayers = Array.from(svg.children).filter(
    (el) =>
      el.classList.contains("dropzone-layer") ||
      el.classList.contains("ghost-qubit-layer"),
  );
  assert.deepEqual(
    directChildLayers,
    [],
    "editor-only layers must not be direct children of svg.qviz",
  );
});

test("trailing-append column lands past the rightmost gate", () => {
  // Locks down the synthesized "past-end" position used for the
  // append-new-column dropzones at top level. If makeDropzoneBox's
  // out-of-bounds colIndex synthesis ever drifts, this catches it.
  const group = singleCircuit({
    qubits: [{ id: 0 }, { id: 1 }],
    componentGrid: [
      {
        components: [{ kind: "unitary", gate: "H", targets: [{ qubit: 0 }] }],
      },
      {
        components: [{ kind: "unitary", gate: "T", targets: [{ qubit: 1 }] }],
      },
    ],
  });

  const { hosts, dropzones } = renderAndCollectGeometry(group);

  // Locate the rightmost host (top-level only — nested gates would
  // start with "0,0-" etc., so a simple substring filter excludes them).
  const topLevelHosts = hosts.filter((h) => !h.location.includes("-"));
  assert.ok(topLevelHosts.length > 0, "expected at least one top-level host");
  const rightmostHostRight = Math.max(
    ...topLevelHosts.map((h) => h.x + h.width),
  );

  // Trailing-append dropzones are tagged inter-column='false' (they
  // act as on-column drops for a brand-new column) but their location
  // colIndex is one past the last actual column. Filter to those.
  const lastTopLevelCol = Math.max(
    ...topLevelHosts.map((h) => Number(h.location.split(",")[0])),
  );
  const trailing = dropzones.filter((d) => {
    const [colStr] = d.location.split(",");
    return Number(colStr) === lastTopLevelCol + 1;
  });
  assert.ok(
    trailing.length > 0,
    `expected trailing-append dropzones at colIndex ${lastTopLevelCol + 1}`,
  );

  // Every trailing dropzone must be centered past the rightmost gate
  // — the band straddles the gap to a hypothetical next column, so
  // its left edge sits roughly `gatePadding` inside the right edge of
  // the last column, but its center (and most of its body) is past it.
  for (const dz of trailing) {
    const dzCenter = dz.x + dz.width / 2;
    assert.ok(
      dzCenter >= rightmostHostRight,
      `trailing dropzone center x=${dzCenter} should be past rightmost gate edge ${rightmostHostRight}`,
    );
    // And it should not extend so far left that it covers most of the
    // last column — its left edge should be at or right of the column's
    // midpoint.
    const lastColMid = topLevelHosts.reduce(
      (max, h) => Math.max(max, h.x + h.width / 2),
      0,
    );
    assert.ok(
      dz.x >= lastColMid,
      `trailing dropzone left x=${dz.x} should not extend left of last column midpoint ${lastColMid}`,
    );
  }
});
