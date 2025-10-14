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

function getContainerElement() {
  const container = document.getElementById("app");
  if (!container) throw new Error("Could not find container element");
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
  const sqore = new Sqore(circuitGroup, {
    renderDepth: 10,
    renderLocation,
  });

  sqore.draw(container);
}

/**
 * @param {{ file: string; line: number; column: number; }} location
 */
function renderLocation(location) {
  // Read the file and extract the specific line
  try {
    const filePath = path.join(__dirname, "..", "cases", location.file);
    const fileContent = fs.readFileSync(filePath, "utf8");
    const lines = fileContent.split("\n");
    const targetLine = lines[location.line] || "";
    const snippet = targetLine.trim();

    // Return a javascript: URL that shows the snippet as alert/tooltip
    return {
      title: `${location.file}:${location.line}:${location.column}\n${snippet.replace(/'/g, "\\'")}`,
      href: "#",
    };
  } catch {
    return {
      title: `Error loading ${location.file}:${location.line}`,
      href: "#",
    };
  }
}

/**
 * @param {import("../../../dist/browser.js").CircuitData} circuitGroup
 * @param {HTMLElement} container
 */
function drawEditableCircuit(circuitGroup, container) {
  const sqore = new Sqore(circuitGroup, {
    isEditable: true,
    renderLocation,
  });

  sqore.draw(container);
}

test("circuit snapshot tests - .qsc files", async (t) => {
  const files = findQscFiles();
  if (files.length === 0) {
    // Not a failure; just informatively skip if there are none.
    t.diagnostic("No .qsc files found under cases/");
    return;
  }

  for (const file of files) {
    const relName = path.basename(file);
    await t.test(relName, (tt) => {
      const circuitGroup = loadCircuitGroup(file);
      drawEditableCircuit(circuitGroup, getContainerElement());
      snapshotHtml(tt, tt.name);
    });
  }
});

test("circuit snapshot tests - .qs files", async (t) => {
  const files = findQsFiles();
  if (files.length === 0) {
    // Not a failure; just informatively skip if there are none.
    t.diagnostic("No .qs files found under cases/");
    return;
  }

  for (const file of files) {
    const relName = path.basename(file);
    const generationMethod = relName.includes("-eval.")
      ? "classicalEval"
      : relName.includes("-static.")
        ? "static"
        : undefined;

    if (generationMethod === undefined) {
      throw new Error(
        `Could not determine generation method from file name ${relName}. Please include -eval or -static in the file name.`,
      );
    }

    await t.test(`${relName}`, async (tt) => {
      const circuitSource = fs.readFileSync(file, "utf8");
      const compiler = getCompiler();
      const circuitGroup = await compiler.getCircuit(
        {
          sources: [[relName, circuitSource]],
          languageFeatures: [],
          profile: "adaptive_rif",
        },
        undefined,
        {
          generationMethod,
          collapseQubitRegisters: false,
          groupScopes: true,
          loopDetection: false,
          maxOperations: 100,
        },
      );

      drawCircuit(circuitGroup, getContainerElement());
      snapshotHtml(tt, tt.name);
    });
  }
});
