// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// State visualizer snapshot tests.
//
// Snapshots are stored as .html files in `test/state-viz-cases/`.
// To (re)generate snapshots:
//   node --test --test-update-snapshots test/stateViz.js

// @ts-check

import { JSDOM } from "jsdom";
import fs from "node:fs";
import path from "node:path";
import { afterEach, beforeEach, test } from "node:test";
import { fileURLToPath } from "node:url";
import prettier from "prettier";
import {
  createStatePanel,
  updateStatePanelFromMap,
} from "../dist/ux/circuit-vis/stateViz.js";
import { draw } from "../dist/ux/circuit-vis/index.js";

const documentTemplate = `<!doctype html><html>
  <head>
    <link rel="stylesheet" href="../../ux/qsharp-ux.css">
    <link rel="stylesheet" href="../../ux/qsharp-circuit.css">
  </head>
  <body></body>
</html>`;

/** @type {JSDOM | null} */
let jsdom = null;

beforeEach(() => {
  jsdom = new JSDOM(documentTemplate, {
    pretendToBeVisual: true,
  });

  // Override the globals used by product code
  // @ts-expect-error - the `jsdom` typings and DOM typings don't match
  globalThis.window = jsdom.window;
  globalThis.document = jsdom.window.document;
  globalThis.Node = jsdom.window.Node;
  globalThis.HTMLElement = jsdom.window.HTMLElement;
  globalThis.SVGElement = jsdom.window.SVGElement;
  globalThis.SVGSVGElement = jsdom.window.SVGSVGElement;
  globalThis.XMLSerializer = jsdom.window.XMLSerializer;
  globalThis.getComputedStyle = jsdom.window.getComputedStyle.bind(
    jsdom.window,
  );
  globalThis.requestAnimationFrame = jsdom.window.requestAnimationFrame.bind(
    jsdom.window,
  );
});

afterEach(() => {
  jsdom?.window.close();
  jsdom = null;
});

function getCasesDirectory() {
  return path.join(
    path.dirname(fileURLToPath(import.meta.url)),
    "state-viz-cases",
  );
}

/** @param {string} file */
function loadCircuit(file) {
  const raw = fs.readFileSync(file, "utf8");
  try {
    return JSON.parse(raw);
  } catch (e) {
    throw new Error(
      `Failed to parse JSON from ${file}: ${/** @type {Error} */ (e).message}`,
    );
  }
}

/** @param {string} id */
function createContainerElement(id) {
  const container = document.createElement("div");
  container.id = id;
  container.className = "qs-circuit";
  document.body.appendChild(container);
  return container;
}

/** @param {number} ms */
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/**
 * Wait for the state viz panel to render a non-trivial state (>= 2 columns),
 * or to settle into an "empty/unsupported" terminal message.
 *
 * @param {HTMLElement} container
 * @param {{ timeoutMs?: number }} [opts]
 */
async function waitForStateVizRendered(container, opts = {}) {
  const timeoutMs = opts.timeoutMs ?? 2500;
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const panel = /** @type {HTMLElement | null} */ (
      container.querySelector(".state-panel")
    );
    if (!panel) {
      await delay(25);
      continue;
    }

    // Expand + disable animations for deterministic snapshots.
    panel.classList.remove("collapsed");
    panel.style.setProperty("--stateAnimMs", "0ms");
    const edge = panel.querySelector(".state-edge");
    edge?.setAttribute("aria-expanded", "true");

    const bars = panel.querySelectorAll(".state-bar");
    if (bars.length >= 2) {
      await delay(0);
      return;
    }

    // If we hit a terminal empty/unsupported state, accept it as resolved.
    const emptyMsg = panel.querySelector(".state-empty-message");
    if (panel.classList.contains("empty") && emptyMsg) {
      await delay(0);
      return;
    }

    await delay(25);
  }
}

/**
 * @param {string} name
 */
function htmlSnapshotPath(name) {
  // Keep snapshots stable across OSes and paths.
  const safe = name.replace(/[^a-zA-Z0-9_.-]+/g, "_");
  return path.join(getCasesDirectory(), safe + ".snapshot.html");
}

/**
 * Check the current document against the stored snapshot.
 * @param {import("node:test").TestContext} t
 * @param {string} name
 */
async function checkDocumentSnapshot(t, name) {
  const rawHtml = new XMLSerializer().serializeToString(document) + "\n";

  await checkHtmlSnapshot(t, name, rawHtml);
}

/**
 * Check an HTML string against the stored snapshot.
 *
 * @param {import("node:test").TestContext} t
 * @param {string} name
 * @param {string} rawHtml
 */
async function checkHtmlSnapshot(t, name, rawHtml) {
  const html = (rawHtml || "").endsWith("\n") ? rawHtml : rawHtml + "\n";
  const formattedHtml = await prettier.format(html, {
    parser: "html",
    printWidth: 80,
    tabWidth: 2,
    useTabs: false,
  });

  t.assert.fileSnapshot(formattedHtml, htmlSnapshotPath(name), {
    serializers: [(s) => String(s)],
  });
}

/**
 * Creates a state panel, attaches it to the DOM, renders an amp map,
 * and returns the panel.
 *
 * @param {Record<string, {re:number, im:number}>} ampMap
 * @param {import("../dist/ux/circuit-vis/stateViz.js").RenderOptions} [opts]
 */
function renderStatePanel(ampMap, opts) {
  const panel = createStatePanel();
  document.body.appendChild(panel);

  // For deterministic snapshots: disable animations.
  panel.style.setProperty("--stateAnimMs", "0ms");

  // Ensure panel is expanded so its contents appear in snapshots.
  panel.classList.remove("collapsed");
  const edge = panel.querySelector(".state-edge");
  edge?.setAttribute("aria-expanded", "true");

  updateStatePanelFromMap(panel, ampMap, opts);
  return panel;
}

test("state viz snapshot - single basis state", async (t) => {
  renderStatePanel({
    0: { re: 1, im: 0 },
  });
  await checkDocumentSnapshot(t, t.name);
});

test("state viz snapshot - superposition with phase", async (t) => {
  const invSqrt2 = Math.SQRT1_2;
  renderStatePanel(
    {
      0: { re: invSqrt2, im: 0 },
      1: { re: 0, im: invSqrt2 }, // phase +π/2
    },
    { normalize: true },
  );
  await checkDocumentSnapshot(t, t.name);
});

test("state viz snapshot - threshold aggregates to Others", async (t) => {
  renderStatePanel(
    {
      "00": { re: 0.94, im: 0 },
      "01": { re: 0.2, im: 0 },
      10: { re: 0.1, im: 0 },
      11: { re: 0.05, im: 0 },
    },
    {
      normalize: true,
      minProbThreshold: 0.05,
      maxColumns: 8,
    },
  );
  await checkDocumentSnapshot(t, t.name);
});

test("state viz integration snapshot", async (t) => {
  const file = path.join(getCasesDirectory(), "state_viz_circuit.qsc");
  const circuit = loadCircuit(file);
  const container = createContainerElement("circuit");

  draw(circuit, container, {
    isEditable: true,
    // Expand the state panel in this integration snapshot.
    statePanelInitiallyExpanded: true,
  });

  await waitForStateVizRendered(container);
  await checkDocumentSnapshot(t, t.name);
});

// Ensure the cases directory exists when running tests in fresh environments.
if (!fs.existsSync(getCasesDirectory())) {
  fs.mkdirSync(getCasesDirectory(), { recursive: true });
}
