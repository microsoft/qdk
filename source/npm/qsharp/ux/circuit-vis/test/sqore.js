// @ts-check
// Discovers *.qsc files and runs each as a subtest.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { withDom } from "./helpers/withDom.js";
import { Sqore } from "../../../dist/ux/circuit-vis/sqore.js";

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
 * Find candidate .qsc files:
 * 1) files next to this test file
 * 2) or anything under a __cases__ folder next to this test file
 */
function findQscFiles() {
  const here = __dirname;
  const candidates = [];

  // A) .qsc files directly in this folder
  for (const name of fs.readdirSync(here)) {
    if (name.toLowerCase().endsWith(".qsc")) {
      candidates.push(path.join(here, name));
    }
  }

  // B) .qsc files under __cases__ (recursively)
  const casesDir = path.join(here, "__cases__");
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
  sqore.draw(container);

  // Write an .html snapshot (update with --test-update-snapshots)
  const outFile = path.join(
    __dirname,
    "__html_snapshots__",
    safe(t.name) + ".html",
  );

  t.assert.fileSnapshot(
    // You may prefer serializeNode(container) if you only want the render root.
    serializeNode(document),
    outFile,
    { serializers: [(s) => String(s)] },
  );
}

/** Make a safe filename from a test name. */
function safe(s) {
  return s
    .toLowerCase()
    .replace(/[^a-z0-9/_-]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

// --- Parent test that spawns one subtest per .qsc file ---
test("sqore .qsc cases", async (t) => {
  const files = findQscFiles();
  if (files.length === 0) {
    // Not a failure; just informatively skip if there are none.
    t.diagnostic("No .qsc files found next to the test or under __cases__/");
    return;
  }

  for (const file of files) {
    const relName = path.basename(file, ".qsc");
    await t.test(relName, (tt) => {
      const circuitGroup = loadCircuitGroup(file);
      checkCircuitSnapshot(circuitGroup, tt);
    });
  }
});
