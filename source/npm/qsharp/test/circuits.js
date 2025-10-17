// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// Circuit snapshot tests: Verifies that Q# circuit diagrams render correctly.
// To add a new test case, add a .qs or .qsc file to `circuits-cases/` and run with
// `node --test --test-update-snapshots` or `npm test -- --test-update-snapshots` to generate the snapshot.
// Snapshots are stored as .html files in `circuits-cases/` and are compared against the rendered output.

// @ts-check

import { createCanvas } from "canvas";
import { JSDOM } from "jsdom";
import fs from "node:fs";
import path from "node:path";
import { afterEach, beforeEach, test } from "node:test";
import { fileURLToPath } from "node:url";
import { getCompiler } from "../dist/main.js";
import { draw } from "../dist/ux/circuit-vis/index.js";

const documentTemplate = `<!doctype html><html>
  <head>
    <link rel="stylesheet" href="../../ux/qsharp-ux.css">
    <link rel="stylesheet" href="../../ux/qsharp-circuit.css">
  </head>
  <body>
  </body>
</html>`;

/** @type {JSDOM | null} */
let jsdom = null;

beforeEach(() => {
  // Create a new test DOM
  jsdom = new JSDOM(documentTemplate);

  // Set up canvas
  // @ts-expect-error - the `canvas` typings and DOM typings don't match
  jsdom.window.HTMLCanvasElement.prototype.getContext = function getContext(
    /** @type {string} */
    type,
    /** @type {any[]} */
    ...args
  ) {
    if (type === "2d") {
      // create a new canvas instance with the same dimensions
      const nodeCanvas = createCanvas(this.width, this.height);
      return nodeCanvas.getContext("2d", ...args);
    }
    return null;
  };

  // Override the globals used by product code
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
 * Create and add a container div to the document body.
 * @param {string} id
 */
function createContainerElement(id) {
  const container = document.createElement("div");
  container.id = id;
  container.className = "qs-circuit";
  document.body.appendChild(container);
  return container;
}

/**
 * Walk a directory recursively, yielding file paths.
 * @param {string} dir
 * @returns {Iterable<string>}
 */
function* walk(dir) {
  if (fs.existsSync(dir) && fs.statSync(dir).isDirectory()) {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) yield* walk(full);
      else yield full;
    }
  }
}

/**
 * Find all files with the given extension under the cases directory.
 * @param {string} ext
 * @param {string} dir
 */
function findFilesWithExtension(dir, ext) {
  const candidates = [];
  for (const f of walk(dir)) {
    if (f.toLowerCase().endsWith(ext)) candidates.push(f);
  }

  // Sort for stable test ordering
  candidates.sort((a, b) => a.localeCompare(b));
  return candidates;
}

/**
 * Get the path to the test cases directory.
 */
function getCasesDirectory() {
  return path.join(
    path.dirname(fileURLToPath(import.meta.url)),
    "circuits-cases",
  );
}

/**
 * Get the path to the HTML snapshot for the given test name.
 * @param {string} name
 */
function htmlSnapshotPath(name) {
  return path.join(getCasesDirectory(), name + ".snapshot.html");
}

/**
 * Check the current document against the stored snapshot.
 * @param {test.TestContext} t
 * @param {string} name
 */
function checkDocumentSnapshot(t, name) {
  t.assert.fileSnapshot(
    new XMLSerializer().serializeToString(document) + "\n",
    htmlSnapshotPath(name),
    {
      serializers: [(s) => String(s)],
    },
  );
}

/**
 * Load a .qsc JSON file and return the parsed circuit.
 * @param {string} file
 * @returns {import("../dist/data-structures/circuit.js").CircuitGroup}
 */
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

test("circuit snapshot tests - .qsc files", async (t) => {
  const files = findFilesWithExtension(getCasesDirectory(), ".qsc");
  if (files.length === 0) {
    t.diagnostic("No .qsc files found under cases");
    return;
  }

  for (const file of files) {
    const relName = path.basename(file);
    await t.test(relName, (tt) => {
      const circuit = loadCircuit(file);
      const container = createContainerElement(`circuit`);
      draw(circuit, container, 0, true /* isEditable */);
      checkDocumentSnapshot(tt, tt.name);
    });
  }
});

test("circuit snapshot tests - .qs files", async (t) => {
  const files = findFilesWithExtension(getCasesDirectory(), ".qs");
  if (files.length === 0) {
    t.diagnostic("No .qs files found under cases");
    return;
  }

  for (const file of files) {
    const relName = path.basename(file);
    await t.test(`${relName}`, async (tt) => {
      const circuitSource = fs.readFileSync(file, "utf8");
      const compiler = getCompiler();
      const container = createContainerElement(`circuit`);
      try {
        // Generate the circuit from Q#
        const circuit = await compiler.getCircuit(
          {
            sources: [[relName, circuitSource]],
            languageFeatures: [],
            profile: "adaptive_rif",
          },
          false,
        );

        // Render the circuit
        draw(circuit, container);
      } catch (e) {
        const pre = document.createElement("pre");
        pre.appendChild(
          document.createTextNode(`Error generating circuit: ${e}`),
        );
        container.appendChild(pre);
      }

      checkDocumentSnapshot(tt, tt.name);
    });
  }
});
