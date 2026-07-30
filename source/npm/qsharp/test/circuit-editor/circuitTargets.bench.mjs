// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// @ts-check

/**
 * Benchmark: render and mutation cost on the circuit editor's
 * `.targets` field on group operations.
 *
 * # Purpose
 *
 * Used as a same-code-path before/after harness when switching
 * between two designs for group `.targets`:
 *
 *   - **today / eager cache**: group ops carry a cached union of
 *     descendant register sets; mutators (`moveOperation` and
 *     friends) walk the ancestor chain and re-run
 *     `getChildTargets` on every affected group.
 *
 *   - **pure derived**: readers (`getMinMaxRegIdx`,
 *     `getOperationRegisters`) descend children when the op is a
 *     group; mutators do nothing extra.
 *
 * Run the same benchmark against each build of `dist/` and
 * compare. The benchmark itself doesn't know which design is
 * loaded — it just times `draw()` and `moveOperation()`.
 *
 * # Columns
 *
 *   - `ops`     — total operations including descendants.
 *   - `grps`    — number of group operations.
 *   - `render`  — median time of one `draw()` call.
 *   - `r p95`   — p95 render time.
 *   - `mutate`  — median time of one `moveOperation` on a
 *                 deeply-nested leaf.
 *   - `m p95`   — p95 mutate time.
 *
 * # Limitations
 *
 *   - Renders happen in JSDOM, which is slower than a real
 *     browser. The *ratio* between two designs is what matters,
 *     not the absolute number.
 *   - Circuit shapes are synthetic but span a realistic range of
 *     (qubits, columns, group density, group nesting depth).
 *
 * # Running
 *
 * Requires the npm `dist/` to be built (`npm run build` in the
 * `source/npm/qsharp/` directory).
 *
 *   node test/circuit-editor/circuitTargets.bench.mjs [label]
 *
 * If `label` is given it's printed at the top of the output so
 * captured runs can be told apart. The `.bench.mjs` extension
 * keeps this file out of `node --test` default discovery.
 */

import { JSDOM } from "jsdom";
import { performance } from "node:perf_hooks";

// ---------------------------------------------------------------------------
// JSDOM setup
// ---------------------------------------------------------------------------
// Must happen before importing anything that touches the renderer.

const jsdom = new JSDOM(`<!doctype html><html><body></body></html>`);
// @ts-expect-error - jsdom typings vs DOM lib mismatch
globalThis.window = jsdom.window;
globalThis.document = jsdom.window.document;
globalThis.Node = jsdom.window.Node;
globalThis.HTMLElement = jsdom.window.HTMLElement;
globalThis.SVGElement = jsdom.window.SVGElement;
globalThis.XMLSerializer = jsdom.window.XMLSerializer;

const { CircuitModel } =
  await import("../../dist/ux/circuit-vis/data/circuitModel.js");
const { moveOperation } =
  await import("../../dist/ux/circuit-vis/actions/circuitActions.js");
const { draw } = await import("../../dist/ux/circuit-vis/index.js");

// ---------------------------------------------------------------------------
// Synthetic circuit builder
// ---------------------------------------------------------------------------

/**
 * @typedef {object} Scenario
 * @property {string} name
 * @property {number} qubits
 * @property {number} columns
 * @property {number} groupRate     0..1, probability a column-slot becomes a group
 * @property {number} groupSize     wires spanned by a group
 * @property {number} nestingDepth  levels of nested groups inside a group
 * @property {number} childCols     non-leaf groups: number of inner columns
 */

/** Deterministic PRNG so runs are comparable. */
function makeRng(seed) {
  let s = seed >>> 0;
  return () => {
    // xorshift32
    s ^= s << 13;
    s ^= s >>> 17;
    s ^= s << 5;
    return ((s >>> 0) % 1000000) / 1000000;
  };
}

/**
 * Build a Circuit (not a CircuitGroup — see `wrap` below).
 * @param {Scenario} opts
 */
function buildCircuit(opts) {
  const { qubits, columns, groupRate, groupSize, nestingDepth, childCols } =
    opts;
  const rng = makeRng(0xc0ffee ^ qubits ^ (columns << 8));

  /** @type {any[]} */
  const componentGrid = [];
  for (let c = 0; c < columns; c++) {
    /** @type {any[]} */
    const column = [];
    let q = 0;
    while (q < qubits) {
      if (rng() < groupRate && q + groupSize <= qubits) {
        const group = buildGroup(q, groupSize, nestingDepth, childCols, rng);
        column.push(group);
        q += groupSize;
      } else {
        column.push({
          kind: "unitary",
          gate: "H",
          targets: [{ qubit: q }],
        });
        q += 1;
      }
    }
    if (column.length > 0) componentGrid.push({ components: column });
  }

  /** @type {any[]} */
  const qubitsArr = [];
  for (let i = 0; i < qubits; i++) qubitsArr.push({ id: i });
  return { qubits: qubitsArr, componentGrid };
}

/**
 * Build a group spanning `size` wires starting at `startWire`,
 * with `depth` levels of nesting inside.
 */
function buildGroup(startWire, size, depth, childCols, rng) {
  /** @type {any[]} */
  const targets = [];
  for (let i = 0; i < size; i++) targets.push({ qubit: startWire + i });

  /** @type {any[]} */
  const childGrid = [];
  for (let c = 0; c < childCols; c++) {
    /** @type {any[]} */
    const col = [];
    if (depth > 0 && rng() < 0.5) {
      // Inner nested group spanning all wires of the parent.
      col.push(buildGroup(startWire, size, depth - 1, childCols, rng));
    } else {
      // Leaf children: one H per wire.
      for (let i = 0; i < size; i++) {
        col.push({
          kind: "unitary",
          gate: "H",
          targets: [{ qubit: startWire + i }],
        });
      }
    }
    childGrid.push({ components: col });
  }

  return {
    kind: "unitary",
    gate: "Foo",
    targets, // eagerly populated (today's design)
    children: childGrid,
  };
}

/** Wrap a Circuit in a CircuitGroup (what `draw` expects). */
function wrap(circuit) {
  return { circuits: [circuit], version: 1 };
}

/**
 * Walk every group in the circuit, calling fn for each.
 * @param {any} circuit
 * @param {(group: any) => void} fn
 */
function forEachGroup(circuit, fn) {
  /** @type {(grid: any[]) => void} */
  const walkGrid = (grid) => {
    for (const col of grid) {
      for (const op of col.components) {
        if (op.children != null) {
          fn(op);
          walkGrid(op.children);
        }
      }
    }
  };
  walkGrid(circuit.componentGrid);
}

function countGroups(circuit) {
  let n = 0;
  forEachGroup(circuit, () => {
    n++;
  });
  return n;
}

function countOps(circuit) {
  let n = 0;
  /** @type {(grid: any[]) => void} */
  const walk = (grid) => {
    for (const col of grid) {
      for (const op of col.components) {
        n++;
        if (op.children != null) walk(op.children);
      }
    }
  };
  walk(circuit.componentGrid);
  return n;
}

/**
 * Find a (sourceLocation, targetLocation) pair that moves a leaf
 * out of a deep group into a sibling slot. Used to give
 * `moveOperation` realistic ancestor-cascade work.
 *
 * Returns null if no suitable structure exists.
 */
function findDeepMove(circuit) {
  for (let c = 0; c < circuit.componentGrid.length; c++) {
    const col = circuit.componentGrid[c];
    for (let o = 0; o < col.components.length; o++) {
      const op = col.components[o];
      if (op.children == null) continue;
      // Find the deepest nested leaf reachable from this group.
      const stack = [{ op, loc: `${c},${o}` }];
      let deepest = null;
      while (stack.length > 0) {
        const { op: cur, loc } = stack.pop();
        if (cur.children == null) continue;
        for (let cc = 0; cc < cur.children.length; cc++) {
          for (let co = 0; co < cur.children[cc].components.length; co++) {
            const child = cur.children[cc].components[co];
            const childLoc = `${loc}-${cc},${co}`;
            if (child.children != null) {
              stack.push({ op: child, loc: childLoc });
            } else {
              deepest = { childLoc, wire: child.targets[0].qubit };
            }
          }
        }
      }
      if (deepest != null) {
        // Move it one column over inside the same outer group.
        // Target: append a new column to the outer group.
        const targetLoc = `${c},${o}-${op.children.length},0`;
        return {
          source: deepest.childLoc,
          target: targetLoc,
          wire: deepest.wire,
        };
      }
    }
  }
  return null;
}

// ---------------------------------------------------------------------------
// Timing helpers
// ---------------------------------------------------------------------------

/**
 * Run `fn` repeatedly and return per-iteration timing stats in
 * milliseconds.
 * @param {() => void} fn
 * @param {{ warmup?: number, iterations: number }} opts
 */
function bench(fn, { warmup = 3, iterations }) {
  for (let i = 0; i < warmup; i++) fn();
  const samples = new Array(iterations);
  for (let i = 0; i < iterations; i++) {
    const t0 = performance.now();
    fn();
    const t1 = performance.now();
    samples[i] = t1 - t0;
  }
  samples.sort((a, b) => a - b);
  const median = samples[Math.floor(iterations / 2)];
  const p95 = samples[Math.floor(iterations * 0.95)];
  const min = samples[0];
  const max = samples[samples.length - 1];
  const sum = samples.reduce((a, b) => a + b, 0);
  return { median, p95, min, max, mean: sum / iterations };
}

/** Pretty-print a millisecond value. */
function fmt(ms) {
  if (ms == null || Number.isNaN(ms)) return "  n/a";
  if (ms >= 100) return ms.toFixed(0) + " ms";
  if (ms >= 1) return ms.toFixed(2) + " ms";
  if (ms >= 0.001) return (ms * 1000).toFixed(1) + " us";
  return (ms * 1000000).toFixed(0) + " ns";
}

// ---------------------------------------------------------------------------
// Scenarios
// ---------------------------------------------------------------------------

/** @type {Scenario[]} */
const scenarios = [
  {
    name: "tiny flat",
    qubits: 5,
    columns: 10,
    groupRate: 0.15,
    groupSize: 2,
    nestingDepth: 0,
    childCols: 1,
  },
  {
    name: "small nested",
    qubits: 5,
    columns: 20,
    groupRate: 0.3,
    groupSize: 2,
    nestingDepth: 2,
    childCols: 2,
  },
  {
    name: "medium flat",
    qubits: 10,
    columns: 50,
    groupRate: 0.2,
    groupSize: 3,
    nestingDepth: 0,
    childCols: 2,
  },
  {
    name: "medium nested",
    qubits: 10,
    columns: 50,
    groupRate: 0.3,
    groupSize: 3,
    nestingDepth: 3,
    childCols: 2,
  },
  {
    name: "large flat",
    qubits: 20,
    columns: 100,
    groupRate: 0.2,
    groupSize: 4,
    nestingDepth: 0,
    childCols: 2,
  },
  {
    name: "large nested",
    qubits: 20,
    columns: 100,
    groupRate: 0.3,
    groupSize: 4,
    nestingDepth: 3,
    childCols: 2,
  },
];

// ---------------------------------------------------------------------------
// Run
// ---------------------------------------------------------------------------

const label = process.argv[2] ?? "unlabeled";

console.log("");
console.log(`Circuit targets benchmark — ${label}`);
console.log(`Node ${process.version}, ${process.platform}-${process.arch}`);
console.log("");

const header = [
  "scenario".padEnd(16),
  "ops".padStart(5),
  "grps".padStart(5),
  "render".padStart(11),
  "r p95".padStart(11),
  "mutate".padStart(11),
  "m p95".padStart(11),
].join(" ");
console.log(header);
console.log("-".repeat(header.length));

for (const scenario of scenarios) {
  const circuit = buildCircuit(scenario);
  const ops = countOps(circuit);
  const groups = countGroups(circuit);

  // ----- Render today (calls real `draw`)
  // Heavy operation; few iterations.
  let renderStats = null;
  try {
    renderStats = bench(
      () => {
        const container = document.createElement("div");
        container.className = "qs-circuit";
        document.body.appendChild(container);
        // Deep clone since `draw` may attach data attrs to ops.
        draw(wrap(JSON.parse(JSON.stringify(circuit))), container, {
          renderDepth: 999999,
        });
        container.remove();
      },
      { warmup: 2, iterations: 10 },
    );
  } catch (e) {
    console.warn(`  render failed for "${scenario.name}": ${e?.message ?? e}`);
  }

  // ----- Mutate (`moveOperation`)
  const move = findDeepMove(circuit);
  let mutateStats = null;
  if (move != null) {
    try {
      mutateStats = bench(
        () => {
          // Deep clone so each iteration mutates an independent
          // circuit. Otherwise the source location is invalidated
          // by the first move.
          const fresh = JSON.parse(JSON.stringify(circuit));
          const model = new CircuitModel(fresh);
          moveOperation(
            model,
            move.source,
            move.target,
            move.wire,
            move.wire,
            false,
            false,
          );
        },
        { warmup: 3, iterations: 100 },
      );
    } catch (e) {
      console.warn(
        `  mutate failed for "${scenario.name}": ${e?.message ?? e}`,
      );
    }
  }

  console.log(
    [
      scenario.name.padEnd(16),
      String(ops).padStart(5),
      String(groups).padStart(5),
      fmt(renderStats?.median).padStart(11),
      fmt(renderStats?.p95).padStart(11),
      fmt(mutateStats?.median).padStart(11),
      fmt(mutateStats?.p95).padStart(11),
    ].join(" "),
  );
}

console.log("");

jsdom.window.close();
