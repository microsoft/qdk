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

/** Serialize the whole document (or a node) for stable SVG/HTML snapshots. */
function serializeNode(node) {
  const ser = new XMLSerializer();
  return ser.serializeToString(node) + "\n";
}

/**
 * Walk a directory recursively and yield file paths.
 * @param {string} dir
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
  const here = __dirname;
  const candidates = [];

  const casesDir = path.join(here, "..", "cases");
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
 * Render and snapshot a CircuitGroup as an .html file.
 * Uses the test's name to generate a friendly file path.
 * @param {import("../../../dist/data-structures/circuit.js").CircuitGroup} circuitGroup
 * @param {import("node:test").TestContext} t
 */
function checkCircuitSnapshot(circuitGroup, t) {
  const container = document.getElementById("app");
  if (!container) throw new Error("Could not find container element");

  // Draw
  const sqore = new Sqore(circuitGroup);

  // TODO: maybe make renderDepth configurable
  sqore.draw(container, 10);

  // Write an .html snapshot (update with --test-update-snapshots)
  const outFile = path.join(__dirname, "..", "cases", t.name + ".html");

  t.assert.fileSnapshot(
    // You may prefer serializeNode(container) if you only want the render root.
    serializeNode(document),
    outFile,
    { serializers: [(s) => String(s)] },
  );
}

// --- Parent test that spawns one subtest per .qsc file ---
test("circuit snapshot tests", async (t) => {
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
      checkCircuitSnapshot(circuitGroup, tt);
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
          sources: [["main.qs", circuitSource]],
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

      checkCircuitSnapshot(circuitGroup, tt);
    });
  }
});
