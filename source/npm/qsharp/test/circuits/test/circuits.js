// @ts-check
// Discovers *.qsc files and runs each as a subtest.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { withDom } from "./helpers/withDom.js";
import { Sqore } from "../../../dist/ux/circuit-vis/sqore.js";
import { getCompiler } from "../../../dist/main.js";

withDom();

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

/**
 * Serialize the whole document (or a node) for stable SVG/HTML snapshots.
 * @param {Document | Node} node
 */
function serializeNode(node) {
  const ser = new XMLSerializer();
  return ser.serializeToString(node) + "\n";
}

/**
 * @param {string} dir
 * @returns {Iterable<string>}
 */
function* walk(dir) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) yield* walk(full);
    else yield full;
  }
}

/**
 * Find candidate .qsc files
 */
function findQscFiles() {
  const here = __dirname;
  const candidates = [];

  const casesDir = path.join(here, "..", "cases");
  if (fs.existsSync(casesDir) && fs.statSync(casesDir).isDirectory()) {
    for (const f of walk(casesDir)) {
      if (f.toLowerCase().endsWith(".qsc")) candidates.push(f);
    }
  }

  // Sort for stable test ordering
  candidates.sort((a, b) => a.localeCompare(b));
  return candidates;
}

/**
 * Find candidate .qs files
 */
function findQsFiles() {
  const candidates = [];

  const casesDir = path.join(__dirname, "..", "cases");
  if (fs.existsSync(casesDir) && fs.statSync(casesDir).isDirectory()) {
    for (const f of walk(casesDir)) {
      if (f.toLowerCase().endsWith(".qs")) candidates.push(f);
    }
  }

  // Sort for stable test ordering
  candidates.sort((a, b) => a.localeCompare(b));
  return candidates;
}

/**
 * Load a .qsc JSON file and return the parsed CircuitGroup.
 * @param {string} file
 * @returns {import("../../../dist/data-structures/circuit.js").CircuitGroup}
 */
function loadCircuitGroup(file) {
  const raw = fs.readFileSync(file, "utf8");
  try {
    return JSON.parse(raw);
  } catch (e) {
    throw new Error(
      `Failed to parse JSON from ${file}: ${/** @type {Error} */ (e).message}`,
    );
  }
}

/**
 * @param {test.TestContext} t
 * @param {string} name
 */
function snapshotHtml(t, name) {
  t.assert.fileSnapshot(serializeNode(document), htmlSnapshotPath(name), {
    serializers: [(s) => String(s)],
  });
}

/**
 * @param {string} id
 */
function getContainerElement(id) {
  const container = document.createElement("div");
  container.id = id;
  container.className = "qs-circuit";
  document.body.appendChild(container);
  return container;
}

/**
 * @param {string} name
 */
function htmlSnapshotPath(name) {
  return path.join(__dirname, "..", "cases", name + ".html");
}

/**
 * @param {import("../../../dist/browser.js").CircuitData} circuitGroup
 * @param {HTMLElement} container
 */
function drawCircuit(circuitGroup, container) {
  new Sqore(circuitGroup).draw(container);
}

/**
 * @param {import("../../../dist/browser.js").CircuitData} circuitGroup
 * @param {HTMLElement} container
 */
function drawEditableCircuit(circuitGroup, container) {
  new Sqore(circuitGroup, true).draw(container);
}

test("circuit snapshot tests - .qsc files", async (t) => {
  const files = findQscFiles();
  if (files.length === 0) {
    t.diagnostic("No .qsc files found under cases");
    return;
  }

  for (const file of files) {
    const relName = path.basename(file);
    await t.test(relName, (tt) => {
      const circuitGroup = loadCircuitGroup(file);
      drawEditableCircuit(circuitGroup, getContainerElement(`circuit`));
      snapshotHtml(tt, tt.name);
    });
  }
});

test("circuit snapshot tests - .qs files", async (t) => {
  const files = findQsFiles();
  if (files.length === 0) {
    t.diagnostic("No .qs files found under cases");
    return;
  }

  for (const file of files) {
    const relName = path.basename(file);

    await t.test(`${relName}`, async (tt) => {
      const circuitSource = fs.readFileSync(file, "utf8");
      const compiler = getCompiler();

      const container = getContainerElement(`circuit`);
      try {
        const circuit = await compiler.getCircuit(
          {
            sources: [[relName, circuitSource]],
            languageFeatures: [],
            profile: "adaptive_rif",
          },
          false,
        );
        drawCircuit(circuit, container);
      } catch (e) {
        const pre = document.createElement("pre");
        pre.appendChild(
          document.createTextNode(`Error generating circuit: ${e}`),
        );
        container.appendChild(pre);
      }

      snapshotHtml(tt, tt.name);
    });
  }
});
