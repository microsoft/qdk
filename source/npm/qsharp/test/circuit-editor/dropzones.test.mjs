// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// Dropzone-layer tests: locks down the location strings emitted by the circuit editor's drop-target
// generator. Covers the drag/drop surface so positioning regressions don't sneak through.
//
// Tests render a small circuit through `draw()` with editor enabled, then inspect the resulting
// `g.dropzone-layer` for the set of `data-dropzone-location` attributes produced. We assert on
// location strings only — pixel positioning is a visual concern not covered here.

// @ts-check

import { JSDOM } from "jsdom";
import { afterEach, beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import { draw } from "../../dist/ux/circuit-vis/index.js";
import { circuit, gate, group } from "./_helpers.mjs";

const documentTemplate = `<!doctype html><html>
  <head></head>
  <body></body>
</html>`;

/** @type {JSDOM | null} */
let jsdom = null;

beforeEach(() => {
  jsdom = new JSDOM(documentTemplate);
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

// No-op editCallback — tests just need the editor branch to run so dropzones are created. Shared
// (read-only) across draw/Sqore calls.
const noopEditor = { editCallback: () => {} };

/**
 * Create a fresh `.qs-circuit` container attached to the document body. `afterEach` tears down the
 * whole JSDOM, so there's nothing to clean up per-container here.
 *
 * @returns {HTMLDivElement}
 */
function makeContainer() {
  const container = document.createElement("div");
  container.className = "qs-circuit";
  document.body.appendChild(container);
  return container;
}

/**
 * Scrape every dropzone descriptor out of an already-rendered container.
 *
 * @param {Element} container
 * @returns {{ location: string; wire: number; interColumn: boolean }[]}
 */
function collectDropzones(container) {
  const rects = container.querySelectorAll(
    "g.dropzone-layer rect.dropzone[data-dropzone-location]",
  );
  return Array.from(rects).map((rect) => ({
    location: rect.getAttribute("data-dropzone-location") ?? "",
    wire: Number(rect.getAttribute("data-dropzone-wire") ?? "-1"),
    interColumn: rect.getAttribute("data-dropzone-inter-column") === "true",
  }));
}

/**
 * Render a CircuitGroup with the editor enabled (so `createDropzones` runs) and return the dropzone
 * descriptors found in the resulting SVG. `renderDepth: 5` forces any group in the input to render
 * expanded (not auto-collapsed).
 *
 * @param {import("../../dist/ux/circuit-vis/index.js").CircuitGroup} circuitGroup
 * @returns {{ location: string; wire: number; interColumn: boolean }[]}
 */
function renderAndCollectDropzones(circuitGroup) {
  const container = makeContainer();
  draw(circuitGroup, container, { editor: noopEditor, renderDepth: 5 });
  return collectDropzones(container);
}

/**
 * Filter dropzones — or any `{ location }` descriptors, e.g. rendered hosts — to those nested under
 * `prefix`. The default `"0,0-"` matches the body of the single top-level `Foo` group used
 * throughout.
 *
 * @template {{ location: string }} T
 * @param {T[]} items
 * @param {string} [prefix]
 * @returns {T[]}
 */
function nestedUnder(items, prefix = "0,0-") {
  return items.filter((d) => d.location.startsWith(prefix));
}

/**
 * Filter dropzones to the trailing band at `location` — the `!interColumn` "append a new column
 * here" rects. Works for both a group's inner band (`"0,0-1,0"`, the default) and the top-level
 * band (`"1,0"`).
 *
 * @param {{ location: string; wire: number; interColumn: boolean }[]} dropzones
 * @param {string} [location]
 */
function trailingBand(dropzones, location = "0,0-1,0") {
  return dropzones.filter((d) => d.location === location && !d.interColumn);
}

/**
 * Render a CircuitGroup through `Sqore` directly (rather than `draw`) so the test can drive
 * `renderCircuit` / `updateCircuit` and inspect `viewState`. Returns the live `Sqore` and its
 * container.
 *
 * @param {import("../../dist/ux/circuit-vis/index.js").CircuitGroup} circuitGroup
 */
async function drawWithSqore(circuitGroup) {
  const { Sqore } = await import("../../dist/ux/circuit-vis/sqore.js");
  const container = makeContainer();
  const sqore = new Sqore(circuitGroup, { editor: noopEditor });
  sqore.draw(container);
  return { sqore, container };
}

/**
 * Click the expand button on the group op rendered at `location`, simulating a user expanding a
 * collapsed group. Asserts the gate and its expand button exist. Returns the gate element.
 *
 * @param {Element} container
 * @param {string} location
 */
function clickExpandButton(container, location) {
  const gate = container.querySelector(`[data-location="${location}"]`);
  assert.ok(gate, `expected gate group at location ${location}`);
  const expandButton = gate.querySelector(".gate-control.gate-expand");
  assert.ok(
    expandButton,
    `expected expand button on collapsed gate at ${location}`,
  );
  // JSDOM's MouseEvent constructor lives on its window.
  const win = /** @type {any} */ (container.ownerDocument.defaultView);
  expandButton.dispatchEvent(new win.MouseEvent("click", { bubbles: true }));
  return gate;
}

/**
 * Assert no dropzone in `dropzones` lands on a wire at or below `boundary` — i.e. on a wire whose
 * index is `>= boundary`. Wires are numbered top-to-bottom (wire 0 at the top), so a higher index
 * sits lower on screen. This is the wire-extent clipping contract for nested bands.
 *
 * @param {{ wire: number }[]} dropzones
 * @param {number} boundary
 * @param {string} label
 */
function assertNoDropzoneAtOrBelowWire(dropzones, boundary, label) {
  const leaked = dropzones.filter((d) => d.wire >= boundary);
  assert.deepEqual(leaked, [], `${label}; leaked: ${JSON.stringify(leaked)}`);
}

/**
 * Build a minimal CircuitGroup wrapping a single Circuit. Keeps the test fixtures readable by
 * hiding the boilerplate.
 *
 * @param {{ qubits: import("../../dist/ux/circuit-vis/index.js").Qubit[];
 *           componentGrid: import("../../dist/ux/circuit-vis/index.js").ComponentGrid; }} circuitData
 * @returns {import("../../dist/ux/circuit-vis/index.js").CircuitGroup}
 */
function singleCircuit(circuitData) {
  return {
    version: 1,
    circuits: [circuitData],
  };
}

// ---------------------------------------------------------------------------
// Baseline: flat circuits emit only top-level (single-segment) dropzone locations.
// ---------------------------------------------------------------------------

test("flat circuit emits only top-level dropzones", () => {
  // Two qubits, two columns: H on q0, then CNOT (control q0, target q1). No groups.
  const cg = singleCircuit(
    circuit(2, [[gate("H", 0)], [gate("X", 1, { ctrls: [0] })]]),
  );

  const dropzones = renderAndCollectDropzones(cg);

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
// Expanded groups: dropzones inside the body carry nested location strings (the parent location
// followed by `-`).
// ---------------------------------------------------------------------------

test("expanded group emits nested-location dropzones inside its body", () => {
  // Custom gate `Foo` containing one nested `H`. Foo is marked expanded via `dataAttributes` so the
  // renderer shows its body.
  const cg = singleCircuit(
    circuit(2, [
      [group("Foo", [[gate("H", 0)]], { expanded: true, span: [0, 1] })],
    ]),
  );

  const dropzones = renderAndCollectDropzones(cg);

  // We expect at least one nested dropzone — one with a location string that starts with the
  // parent's "0,0" prefix (the only top-level op).
  const nested = nestedUnder(dropzones);
  assert.ok(
    nested.length > 0,
    `expected nested dropzones inside expanded Foo group, got locations: ${JSON.stringify(
      dropzones.map((d) => d.location),
    )}`,
  );
});

// ---------------------------------------------------------------------------
// Wire-extent clipping: an expanded group that spans only some wires must not emit nested dropzones
// on wires outside its extent — the data model can't represent a drop into Foo on a wire Foo
// doesn't already cover without silently widening Foo's targets.
// ---------------------------------------------------------------------------

test("nested dropzones are clipped to the group's wire extent", () => {
  // 3 qubits. Foo only spans wires 0-1; wire 2 has its own X gate sitting alongside (so the
  // renderer keeps wire 2 visible).
  const cg = singleCircuit(
    circuit(3, [
      [
        group("Foo", [[gate("H", 0)]], { expanded: true, span: [0, 1] }),
        gate("X", 2),
      ],
    ]),
  );

  const dropzones = renderAndCollectDropzones(cg);
  const nested = nestedUnder(dropzones);

  // Nested dropzones must exist — otherwise the clipping assertion below is vacuously true.
  assert.ok(
    nested.length > 0,
    "expected some nested dropzones inside expanded Foo group",
  );

  // None of them may target wire 2 (outside Foo's [0, 1] extent).
  assertNoDropzoneAtOrBelowWire(
    nested,
    2,
    "nested dropzones must be clipped to Foo's wire extent (wires 0-1)",
  );
});

// ---------------------------------------------------------------------------
// Nested dropzones must appear when a group is rendered expanded by the renderer (via `renderDepth`
// or expand-button click) even when the source op has no pre-baked `dataAttributes.expanded` flag.
//
// `Sqore.renderCircuit` deep-copies the circuit and runs `expandOperationsToDepth` /
// `expandIfSingleOperation` on the copy. The dropzone recursion iterates
// `sqore.circuit.componentGrid` (the original), so it must be driven by the LayoutMap (built from
// the copy), not by the source op's flag.
// ---------------------------------------------------------------------------

test("nested dropzones appear when expansion is render-time only (not pre-baked)", () => {
  // Foo has children but no `dataAttributes.expanded`. Render-time expansion comes from
  // `renderDepth: 5` in the helper.
  const cg = singleCircuit(
    circuit(2, [[group("Foo", [[gate("H", 0)]], { span: [0, 1] })]]),
  );

  const dropzones = renderAndCollectDropzones(cg);

  const nested = nestedUnder(dropzones);
  assert.ok(
    nested.length > 0,
    `expected nested dropzones inside render-time-expanded Foo group, got locations: ${JSON.stringify(
      dropzones.map((d) => d.location),
    )}`,
  );
});

// ---------------------------------------------------------------------------
// Persistent view state: a user-initiated expand (expand-button click) must survive subsequent
// re-renders, including those triggered by editor mutations. `ViewState` decouples view preferences
// from the saved circuit so they survive the per-render deep copy.
// ---------------------------------------------------------------------------

test("user expand choice survives a re-render via ViewState", async () => {
  // 2 columns prevents `expandIfSingleOperation` from auto-expanding; Foo starts collapsed.
  const cg = singleCircuit(
    circuit(3, [
      [group("Foo", [[gate("H", 0)]], { span: [0, 1] })],
      [gate("X", 2)],
    ]),
  );

  // Construct Sqore directly so the test can call renderCircuit to simulate an editor-mutation
  // refresh.
  const { sqore, container } = await drawWithSqore(cg);

  const collectNested = () => nestedUnder(collectDropzones(container));

  // Sanity: Foo starts collapsed.
  assert.equal(
    collectNested().length,
    0,
    "Foo should start collapsed (no auto-expand applies to a multi-column grid)",
  );

  // Find the expand button and click it. The Sqore handler writes to viewState and triggers a
  // re-render.
  clickExpandButton(container, "0,0");

  // After click: Foo is expanded; nested dropzones appear.
  assert.ok(
    collectNested().length > 0,
    "Foo should be expanded after expand-button click",
  );

  // Verify the user choice was recorded in viewState.
  assert.equal(
    sqore.viewState.expanded.get("0,0"),
    true,
    "viewState should record the user's expand choice",
  );

  // Simulate an editor-mutation refresh — the same path the editor controllers use after every
  // Action.
  /** @type {any} */ (sqore).renderCircuit(container);

  // ViewState's `applyTo` re-applies the user override across the deep-copy boundary, so Foo stays
  // expanded.
  assert.ok(
    collectNested().length > 0,
    "Foo's expand state must survive the editor-mutation re-render",
  );
  assert.equal(
    sqore.viewState.expanded.get("0,0"),
    true,
    "viewState entry must remain after re-render",
  );
});

// ---------------------------------------------------------------------------
// Drag-causes-shift integration test: dragging a gate (or any edit that splices a new column into
// the top-level grid) shifts an expanded group's location string (e.g. "1,0" → "2,0"). `Sqore`
// rebases viewState keys by object identity at the start of every render, so user expand/collapse
// choices follow their op across position shifts.
// ---------------------------------------------------------------------------

test("user expand choice survives an upstream column-shift mutation", async () => {
  // Top-level grid: [col 0: X on q2] [col 1: Foo group with H inside]. Foo lives at "1,0". The test
  // will splice a new column at index 0, shifting Foo to "2,0".
  const cg = singleCircuit(
    circuit(3, [
      [gate("X", 2)],
      [group("Foo", [[gate("H", 0)]], { span: [0, 1] })],
    ]),
  );

  const { sqore, container } = await drawWithSqore(cg);

  // User expands Foo (at "1,0") via the expand button.
  clickExpandButton(container, "1,0");
  assert.equal(
    sqore.viewState.expanded.get("1,0"),
    true,
    "viewState should record the user's expand choice at 1,0",
  );

  // Simulate an editor mutation that inserts a new column at index 0 of the top-level grid. This is
  // the canonical drag-out-of-group shape: a gate dropped into a fresh column ahead of the group
  // pushes the group's column index from 1 to 2.
  sqore.circuit.componentGrid.splice(0, 0, {
    components: [
      {
        kind: "unitary",
        gate: "Y",
        targets: [{ qubit: 0 }],
      },
    ],
  });

  // Re-render via the same path the editor controllers use after every Action.
  /** @type {any} */ (sqore).renderCircuit(container);

  // The identity-based rebase moves the viewState entry from "1,0" to "2,0" along with the op, so
  // Foo stays expanded at its new location.
  assert.equal(
    sqore.viewState.expanded.has("1,0"),
    false,
    "stale viewState key at old location must be cleared",
  );
  assert.equal(
    sqore.viewState.expanded.get("2,0"),
    true,
    "viewState entry must follow the op to its new location",
  );

  // And the visible side of the contract: Foo is still drawn expanded.
  const fooGateAfter = container.querySelector('[data-location="2,0"]');
  assert.ok(fooGateAfter, "expected Foo gate at new location 2,0");
  assert.equal(
    fooGateAfter.getAttribute("data-expanded"),
    "true",
    "Foo must still render as expanded after the column shift",
  );
});

// ---------------------------------------------------------------------------
// External circuit update via `Sqore.updateCircuit`: models the VS Code undo/redo path. The host
// parses a new `CircuitGroup` from the reverted text and pushes it down. `updateCircuit` swaps the
// active circuit in place so the same `Sqore` (and therefore `viewState`) survives.
// ---------------------------------------------------------------------------

test("user expand choice survives an external circuit update via updateCircuit", async () => {
  const buildGroup = () =>
    singleCircuit(
      circuit(3, [
        [group("Foo", [[gate("H", 0)]], { span: [0, 1] })],
        [gate("X", 2)],
      ]),
    );

  const { sqore, container } = await drawWithSqore(buildGroup());

  const collectNested = () => nestedUnder(collectDropzones(container));

  // Sanity: Foo starts collapsed.
  assert.equal(
    collectNested().length,
    0,
    "Foo should start collapsed (no auto-expand applies to a multi-column grid)",
  );

  // User expands Foo via the expand button.
  clickExpandButton(container, "0,0");
  assert.ok(
    collectNested().length > 0,
    "Foo should be expanded after expand-button click",
  );

  // Capture the SVG identity to verify `updateCircuit` does an in-place swap (`replaceChild`)
  // rather than wiping the container.
  const svgBefore = container.querySelector("svg.qviz");
  assert.ok(svgBefore, "expected an svg.qviz element to be rendered");

  // Simulate the host pushing a new (logically equivalent) circuit down — the shape the VS Code
  // editor would build after undo/redo or an external file edit.
  sqore.updateCircuit(buildGroup());

  // Foo's user expand choice must still apply to the new circuit.
  assert.ok(
    collectNested().length > 0,
    "Foo must remain expanded after updateCircuit",
  );
  assert.equal(
    sqore.viewState.expanded.get("0,0"),
    true,
    "viewState entry must survive updateCircuit",
  );

  // The container itself is the same DOM node (no innerHTML wipe); the SVG was swapped in via
  // replaceChild.
  assert.ok(
    container.querySelector("svg.qviz"),
    "container must still contain an svg.qviz after updateCircuit",
  );
});

// ---------------------------------------------------------------------------
// Pixel-coordinate tests: for every rendered gate, the on-column dropzone with the matching
// `data-dropzone-location` must cover the gate's x range. Dropping a gate on top of an existing
// gate lands on a real dropzone.
// ---------------------------------------------------------------------------

/**
 * Render a CircuitGroup with the editor enabled and return both the rendered host elements (the
 * gate boxes) and the produced dropzones, each annotated with bounding-box coordinates pulled from
 * SVG attrs.
 *
 * @param {import("../../dist/ux/circuit-vis/index.js").CircuitGroup} circuitGroup
 */
function renderAndCollectGeometry(circuitGroup) {
  const container = makeContainer();

  draw(circuitGroup, container, { editor: noopEditor, renderDepth: 5 });

  // The gate body box carries `data-width` and `x` in absolute SVG coords. `data-location` lives on
  // a parent `<g class="gate">` (set via dataAttributes spread). So: find every box, walk up to the
  // closest `[data-location]`. Skip control circles / swap markers (those are sibling elements, not
  // the canonical body).
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
        location: gateGroup?.getAttribute("data-location") ?? "",
        x: Number(el.getAttribute("x") ?? "NaN"),
        width: Number(el.getAttribute("data-width") ?? "NaN"),
      };
    })
    .filter((h) => h.location !== "");

  // Dropzones — every on-column rect (interColumn=false) carries a `data-dropzone-location` whose
  // value matches a host's location.
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
 * Parse a hierarchical location string into its scope prefix and the (colIndex, opIndex) inside
 * that scope.
 *
 *   "0,0"     -> { prefix: "",     colIndex: 0, opIndex: 0 } "0,0-1,2" -> { prefix: "0,0",
 *   colIndex: 1, opIndex: 2 }
 */
function parseLocation(/** @type {string} */ loc) {
  const lastDash = loc.lastIndexOf("-");
  const prefix = lastDash === -1 ? "" : loc.slice(0, lastDash);
  const tail = lastDash === -1 ? loc : loc.slice(lastDash + 1);
  const [colIndex, opIndex] = tail.split(",").map(Number);
  return { prefix, colIndex, opIndex };
}

/**
 * For each rendered gate `host`, assert there is at least one on-column dropzone in the *same
 * column* (same prefix + colIndex) whose x-range overlaps the host's x-range. This is the geometry
 * property that actually matters for the editor: dropping near a gate must hit a dropzone in that
 * gate's column.
 *
 * Note: we deliberately match on (prefix, colIndex) — NOT full location — because a column with N
 * ops emits dropzones at opIndex `0..N` (slots above each op + the trailing slot). Every dropzone
 * in that column has the same x/width as every other (they're all sized to the column), so any one
 * of them is a valid coverage witness.
 */
function assertHostsCoveredByColumnDropzones(
  /** @type {{location: string, x: number, width: number}[]} */ hosts,
  /** @type {{location: string, x: number, width: number, wire: number}[]} */ dropzones,
  /** @type {string} */ label,
) {
  for (const host of hosts) {
    const hostKey = parseLocation(host.location);
    const sameCol = dropzones.filter((/** @type {{location: string}} */ d) => {
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
  const cg = singleCircuit(
    circuit(2, [
      [gate("H", 0)],
      [gate("T", 1)],
      [gate("X", 1, { ctrls: [0] })],
    ]),
  );

  const { hosts, dropzones } = renderAndCollectGeometry(cg);

  assert.ok(hosts.length > 0, "expected at least some host elements");
  assertHostsCoveredByColumnDropzones(hosts, dropzones, "flat circuit");
});

test("expanded group: nested gates are covered by their on-column dropzones", () => {
  // Nested gates emit dropzones with locations like `0,0-…`. Each must overlap the gate's x range.
  const cg = singleCircuit(
    circuit(2, [
      [group("Foo", [[gate("H", 0)], [gate("X", 1)]], { expanded: true })],
    ]),
  );

  const { hosts, dropzones } = renderAndCollectGeometry(cg);

  // Filter to nested hosts — the gates inside Foo's body. Their location strings start with "0,0-".
  const nestedHosts = nestedUnder(hosts);
  assert.ok(
    nestedHosts.length > 0,
    `expected some nested host elements inside Foo, got hosts: ${JSON.stringify(
      hosts.map((h) => h.location),
    )}`,
  );

  assertHostsCoveredByColumnDropzones(nestedHosts, dropzones, "expanded group");
});

// ---------------------------------------------------------------------------
// Editor overlay structure: all editor-only DOM (dropzones, ghost qubit row, future overlays) lives
// inside a single `g.editor-overlay` group attached to `svg.qviz`. The renderer never touches the
// overlay; the editor never appends outside it.
// ---------------------------------------------------------------------------

test("editor-only DOM lives inside the editor-overlay group", () => {
  const cg = singleCircuit(
    circuit(2, [[gate("H", 0)], [gate("X", 1, { ctrls: [0] })]]),
  );

  const container = makeContainer();
  draw(cg, container, { editor: noopEditor });

  const svg = container.querySelector("svg.qviz");
  assert.ok(svg, "expected an svg.qviz");

  // Exactly one editor-overlay group, attached as a direct child of svg.qviz.
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

  // No editor-only layers may exist as direct children of svg.qviz outside the overlay. Walk svg's
  // direct children and verify.
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
  // Locks down the synthesized "past-end" position used for the append-new-column dropzones at top
  // level. If makeDropzoneBox's out-of-bounds colIndex synthesis ever drifts, this catches it.
  const cg = singleCircuit(circuit(2, [[gate("H", 0)], [gate("T", 1)]]));

  const { hosts, dropzones } = renderAndCollectGeometry(cg);

  // Locate the rightmost host (top-level only — nested gates would start with "0,0-" etc., so a
  // simple substring filter excludes them).
  const topLevelHosts = hosts.filter((h) => !h.location.includes("-"));
  assert.ok(topLevelHosts.length > 0, "expected at least one top-level host");
  const rightmostHostRight = Math.max(
    ...topLevelHosts.map((h) => h.x + h.width),
  );

  // Trailing-append dropzones are tagged inter-column='false' (they act as on-column drops for a
  // brand-new column) but their location colIndex is one past the last actual column. Filter to
  // those.
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

  // Every trailing dropzone must be centered past the rightmost gate — the band straddles the gap
  // to a hypothetical next column, so its left edge sits roughly `gatePadding` inside the right
  // edge of the last column, but its center (and most of its body) is past it.
  for (const dz of trailing) {
    const dzCenter = dz.x + dz.width / 2;
    assert.ok(
      dzCenter >= rightmostHostRight,
      `trailing dropzone center x=${dzCenter} should be past rightmost gate edge ${rightmostHostRight}`,
    );
    // And it should not extend so far left that it covers most of the last column — its left edge
    // should be at or right of the column's midpoint.
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

// ---------------------------------------------------------------------------
// Expanded groups: trailing inner-column dropzone band on the right edge of the group's body — the
// "extend the group sideways to swallow one more column" gesture. The dropzone's location string
// identifies the new column as belonging to the group's own scope.
// ---------------------------------------------------------------------------

test("expanded group emits a trailing inner-column dropzone band on its right edge", () => {
  // `Foo` spans wires 0-1 with two children in column 0. The trailing inner-column band sits at the
  // synthesized past-end column index `1`, prefixed by Foo's location `0,0` — location string
  // `0,0-1,0`.
  const cg = singleCircuit(
    circuit(2, [
      [group("Foo", [[gate("H", 0), gate("Y", 1)]], { expanded: true })],
    ]),
  );

  const dropzones = renderAndCollectDropzones(cg);

  // Trailing inner-column dropzones are tagged `data-dropzone-inter-column="false"` (drop, don't
  // insert-between) and live at colIndex == childrenColumnCount inside Foo's scope.
  const innerTrailing = trailingBand(dropzones);
  assert.equal(
    innerTrailing.length,
    2,
    `expected one trailing inner-column dropzone per wire Foo spans (2),` +
      ` got locations: ${JSON.stringify(
        nestedUnder(dropzones).map((d) => `${d.location}@w${d.wire}`),
      )}`,
  );

  // Wires must be exactly Foo's span (0 and 1), no leakage to wire 2 or above (defensive — no wire
  // 2 in this fixture, but the clamp contract should hold).
  const wires = innerTrailing.map((d) => d.wire).sort();
  assert.deepEqual(
    wires,
    [0, 1],
    "trailing inner-column dropzones must cover exactly Foo's wire span",
  );
});

test("trailing inner-column dropzones are clipped to the group's wire extent", () => {
  // Foo spans wires 0-1 only; a sibling X on wire 2 keeps that wire visible. Foo's trailing
  // inner-column band must not leak onto wire 2.
  const cg = singleCircuit(
    circuit(3, [
      [
        group("Foo", [[gate("H", 0)]], { expanded: true, span: [0, 1] }),
        gate("X", 2),
      ],
    ]),
  );

  const dropzones = renderAndCollectDropzones(cg);

  // Inner-trailing band: `!interColumn` at location `0,0-1,0`.
  const innerTrailing = trailingBand(dropzones);

  // Must exist — otherwise the clipping assertion below is vacuously true.
  assert.ok(
    innerTrailing.length > 0,
    "expected trailing inner-column dropzones inside Foo to be emitted",
  );

  assertNoDropzoneAtOrBelowWire(
    innerTrailing,
    2,
    "trailing inner-column dropzones must be clipped to Foo's wire span",
  );
});

test("collapsed group emits no inner dropzones", () => {
  // A collapsed group has no `LayoutMap` scope entry, so the dropzone recursion never enters it and
  // the trailing-column helper never runs. No inner-scope dropzones should leak out.
  //
  // The sibling top-level op pins Foo collapsed: without it, `expandIfSingleOperation` would
  // auto-expand Foo when it's the only op at the top level.
  const cg = singleCircuit(
    circuit(2, [
      [group("Foo", [[gate("H", 0)]], { span: [0, 1] })],
      [gate("X", 0)],
    ]),
  );

  // Render with renderDepth: 0 so Foo stays collapsed. We can't use the helper (it forces
  // renderDepth: 5), so inline the draw call.
  const container = makeContainer();
  draw(cg, container, { editor: noopEditor, renderDepth: 0 });

  const nested = collectDropzones(container).filter((d) =>
    d.location.includes("-"),
  );
  assert.deepEqual(
    nested,
    [],
    `collapsed Foo should not emit nested-location dropzones (trailing` +
      ` band included), got: ${JSON.stringify(nested.map((d) => d.location))}`,
  );
});

test("top-level trailing-column band is not clipped to any group's wire span", () => {
  // The top-level trailing band must cover every wire, not just the wires of any group inside it. A
  // wire-clamp regression would restrict the band to a subset of `[0, wireData.length)`.
  const cg = singleCircuit(
    circuit(3, [[gate("H", 0), gate("X", 1), gate("Y", 2)]]),
  );

  const dropzones = renderAndCollectDropzones(cg);

  // One column at top level, so trailing colIndex is 1. Location "1,0" is the trailing band's
  // location (no prefix). The editor also renders a ghost-qubit row at wire index `wireData.length`
  // for the add-a-qubit affordance, so the top-level trailing band covers more wires than the
  // circuit declares. Assert wires {0, 1, 2} are present rather than nailing the exact count.
  const topTrailing = trailingBand(dropzones, "1,0");
  const wires = new Set(topTrailing.map((d) => d.wire));
  for (const w of [0, 1, 2]) {
    assert.ok(
      wires.has(w),
      `top-level trailing band must cover wire ${w}; got wires ${JSON.stringify(
        [...wires].sort(),
      )}`,
    );
  }
});
