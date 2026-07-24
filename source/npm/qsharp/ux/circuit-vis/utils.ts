// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { ComponentGrid, Operation } from "./data/circuit.js";
import { Location } from "./data/location.js";
import { Register } from "./data/register.js";

/**
 * Performs a deep equality check between two objects or arrays.
 * @param obj1 - The first object or array to compare.
 * @param obj2 - The second object or array to compare.
 * @returns True if the objects are deeply equal, false otherwise.
 */
const deepEqual = (obj1: unknown, obj2: unknown): boolean => {
  if (obj1 === obj2) return true;

  if (
    obj1 === null ||
    obj2 === null ||
    typeof obj1 !== "object" ||
    typeof obj2 !== "object"
  ) {
    return false;
  }

  const keys1 = Object.keys(obj1);
  const keys2 = Object.keys(obj2);

  if (keys1.length !== keys2.length) return false;

  for (const key of keys1) {
    if (
      !keys2.includes(key) ||
      !deepEqual(
        (obj1 as Record<string, unknown>)[key],
        (obj2 as Record<string, unknown>)[key],
      )
    ) {
      return false;
    }
  }

  return true;
};

/**
 * Find targets of an operation's children by recursively walking through all of its children's
 * controls, targets, and (for measurements) qubits + results. Note that this intentionally ignores
 * the direct targets of `operation` itself; it's the union of the *descendants'* register sets.
 *
 * Used by the action layer to refresh a group's cached `.targets`/`.results` after its children
 * have been mutated (see `moveOperation` and `_pruneEmptyAncestors` in
 * [`actions/circuitActions.ts`](actions/circuitActions.ts)).
 *
 * Output registers are deduplicated by **full register identity** — the `(qubit, result)` tuple —
 * not by `qubit` alone. A bare-qubit ref `{qubit: 0}` and a classical-register ref `{qubit: 0,
 * result: 0}` are distinct and both survive if both appear among the descendants. Preserving
 * `result` keeps the classical-control visual-extent line that classically-conditional unitaries
 * record in `targets` from being lost on refresh.
 *
 * @param operation The operation to find targets for.
 * @returns An array of registers with unique `(qubit, result)` identities; `result` is preserved
 *   when present.
 */
const getChildTargets = (operation: Operation): Register[] | [] => {
  const _recurse = (operation: Operation) => {
    switch (operation.kind) {
      case "measurement":
        registers.push(...operation.qubits);
        registers.push(...operation.results);
        break;
      case "unitary":
        registers.push(...operation.targets);
        if (operation.controls) {
          registers.push(...operation.controls);
        }
        break;
      case "ket":
        registers.push(...operation.targets);
        break;
    }

    // If there is more children, keep adding more to registers
    if (operation.children) {
      operation.children.forEach((col) =>
        col.components.forEach((child) => {
          _recurse(child);
        }),
      );
    }
  };

  const registers: Register[] = [];
  if (operation.children == null) return [];

  // Recursively walkthrough all children to populate registers
  operation.children.forEach((col) =>
    col.components.forEach((child) => {
      _recurse(child);
    }),
  );

  // Dedup by full register identity (qubit + result). `undefined` result and an explicit result are
  // distinct register kinds (see dedup contract in the doc comment); we use a unique sentinel in
  // the key to avoid collisions like `qubit=0, result=undefined` vs `qubit=0:undefined-as-string`.
  const seen = new Set<string>();
  const out: Register[] = [];
  for (const reg of registers) {
    const key =
      reg.result === undefined
        ? `${reg.qubit}:q`
        : `${reg.qubit}:c${reg.result}`;
    if (seen.has(key)) continue;
    seen.add(key);
    // Rebuild fresh objects rather than aliasing the descendants' own register references — callers
    // assign the returned array straight into `parent.targets`/`.results`, and we don't want a
    // later mutation on a child's register to mutate the parent's cached extent.
    out.push(
      reg.result === undefined
        ? { qubit: reg.qubit }
        : { qubit: reg.qubit, result: reg.result },
    );
  }
  return out;
};

/**
 * Gets the location of an operation, if it has one.
 *
 * @param operation The operation to get the location for.
 * @returns The location string of the operation, or null if it doesn't have one.
 */
const getGateLocationString = (operation: Operation): string | null => {
  if (operation.dataAttributes == null) return null;
  return operation.dataAttributes["location"];
};

/**
 * Get the minimum and maximum drawn-row indices for a given operation. Bare qubit row `q` is index
 * `q`; classical-result rows (registers with `.result !== undefined`) sit BELOW their owning qubit
 * row and are encoded as `q + 0.5` so the inclusive range correctly distinguishes them from a
 * quantum entry at `q + 1` and from a quantum entry at `q` itself.
 *
 * Used exclusively for sibling-overlap checks (`_doesOverlap`) in the action layer. Callers that
 * need integer wire indices should use `getQuantumWireRange` instead.
 *
 * @param operation The operation for which to get the row indices.
 * @returns A tuple containing the minimum and maximum row indices.
 */
function getMinMaxRegIdx(operation: Operation): [number, number] {
  // Classical-register rows sit immediately below their owning qubit row (between qubit `q` and
  // qubit `q+1`). Encode them as `q + 0.5` so the inclusive-range overlap check correctly
  // distinguishes a quantum row at `q` from a classical row of `q`.
  const rows = getOperationRegisters(operation).map((r) =>
    r.result !== undefined ? r.qubit + 0.5 : r.qubit,
  );
  const minRegIdx: number = Math.min(...rows);
  const maxRegIdx: number = Math.max(...rows);

  return [minRegIdx, maxRegIdx];
}

/**
 * Like `getMinMaxRegIdx`, but excludes classical-control registers (those whose `.result` is set).
 * The qubit field of a classical control points at the producing measurement's qubit wire, which
 * isn't really "part of" the consumer op's body — it's just a back-reference used to draw the
 * connector down to the classical wire row.
 *
 * Use this for any decision about which wires belong to an op's editable scope: child-drop scope of
 * an expanded group, shift-extend reach of a parent group, multi-leg drop targets for a selected
 * op. Using `getMinMaxRegIdx` for those would wrongly sweep in the producing measurement's qubit
 * wire.
 *
 * Returns `[-1, -1]` if the op has no quantum-only registers (shouldn't happen for any valid op,
 * but defensive).
 */
const getQuantumWireRange = (operation: Operation): [number, number] => {
  const qRegs = getOperationRegisters(operation).filter(
    ({ result }) => result === undefined,
  );
  if (qRegs.length === 0) return [-1, -1];
  const qRegIdxList = qRegs.map(({ qubit }) => qubit);
  return [Math.min(...qRegIdxList), Math.max(...qRegIdxList)];
};

/**
 * Get every `Register` referenced by an operation, including both its controls and its
 * targets/qubits/results. Returned references are the live objects on the operation, so callers may
 * mutate `reg.qubit` / `reg.result` in place to renumber wires.
 *
 * Mirrors the union that `getMinMaxRegIdx` walks; centralized here so the action layer and the data
 * layer don't each re-spell the per-`kind` switch.
 *
 * @param operation The operation to enumerate registers for.
 * @returns All registers (controls + targets/qubits/results) of `operation`.
 */
const getOperationRegisters = (operation: Operation): Register[] => {
  let targets: Register[];
  let controls: Register[];
  switch (operation.kind) {
    case "measurement":
      targets = operation.results;
      controls = operation.qubits;
      break;
    case "unitary":
      targets = operation.targets;
      controls = operation.controls || [];
      break;
    case "ket":
      targets = operation.targets;
      controls = [];
      break;
  }
  return [...controls, ...targets];
};

/**
 * Vertical extent of an op on the wire grid, expressed as a pair of `Register` endpoints. Either
 * endpoint may land on a qubit row (no `.result`) or on a classical-result row (`.result` set).
 *
 * Geometry. Classical-result rows sit immediately BELOW their owning qubit row — the rendered stack
 * on a qubit `q_c` with results `r0..rN` reads, top to bottom:
 *
 *     q_c,  q_c.r0,  q_c.r1, ...,  q_c.rN,  q_(c+1), ...
 *
 * Endpoints are compared by `.qubit` first, then by `.result` (with `undefined` sorting above any
 * concrete result index, i.e. the bare qubit row sits above its own classical-result rows).
 *
 * The returned endpoints are FRESH register objects (not aliases of the op's own registers) so
 * callers can keep them around without risking mutation of the source op.
 *
 * Returns `null` if the op carries no registers at all (defensive — no valid op should hit this).
 *
 * @param operation The operation to measure.
 * @returns `[min, max]` register endpoints, or `null` if empty.
 */
const getWireRange = (operation: Operation): [Register, Register] | null => {
  const regs = getOperationRegisters(operation);
  if (regs.length === 0) return null;
  // `result === undefined` sorts ABOVE any concrete result index on the same qubit (bare qubit row
  // sits above its r0 row). Encode `undefined` as -1 for the comparison key.
  const key = (r: Register) => r.result ?? -1;
  let minReg = regs[0];
  let maxReg = regs[0];
  for (let i = 1; i < regs.length; i++) {
    const r = regs[i];
    if (
      r.qubit < minReg.qubit ||
      (r.qubit === minReg.qubit && key(r) < key(minReg))
    ) {
      minReg = r;
    }
    if (
      r.qubit > maxReg.qubit ||
      (r.qubit === maxReg.qubit && key(r) > key(maxReg))
    ) {
      maxReg = r;
    }
  }
  // Copy so callers can't mutate the op's own registers.
  const copy = (r: Register): Register =>
    r.result === undefined
      ? { qubit: r.qubit }
      : { qubit: r.qubit, result: r.result };
  return [copy(minReg), copy(maxReg)];
};

/**********************
 *  Finder Functions  *
 * *********************/

/**
 * Walk a path of `[colIdx, opIdx]` segments from a root grid down through nested operation
 * children, returning the grid reached at the end.
 *
 * Returns `null` if any segment is out of bounds — for example because the model has changed since
 * the location was captured (a stale `data-location` attribute on a DOM node, or a hand-constructed
 * location that addresses an op that no longer exists).
 *
 * Note: matches the long-standing semantic that an interior op missing a `children` array does
 * *not* fail the walk; the walk stays on the same grid for that step. Out-of-bounds is the only
 * thing that produces `null`.
 */
const _walkToGrid = (
  componentGrid: ComponentGrid,
  segments: ReadonlyArray<readonly [number, number]>,
): ComponentGrid | null => {
  let grid = componentGrid;
  for (const [colIdx, opIdx] of segments) {
    const col = grid[colIdx];
    if (col == null) return null;
    const op = col.components[opIdx];
    if (op == null) return null;
    grid = op.children ?? grid;
  }
  return grid;
};

/**
 * Find the parent operation of the operation specified by location.
 *
 * Navigates via [`Location`](data/location.ts) so the addressing format is owned by exactly one
 * module.
 *
 * @param componentGrid The grid of components to search through.
 * @param location The location string of the operation.
 * @returns The parent operation, or `null` if the location is empty, shallower than two segments,
 *   or addresses an op that does not exist.
 */
const findParentOperation = (
  componentGrid: ComponentGrid,
  location: string | null,
): Operation | null => {
  if (!location) return null;

  const parsed = Location.parse(location);
  // Need at least two segments: one for the op itself, one for its parent.
  if (parsed.depth < 2) return null;

  const parentOpLocation = parsed.parent();
  const parentOpSegment = parentOpLocation.last();
  if (parentOpSegment == null) return null;

  const grid = _walkToGrid(componentGrid, parentOpLocation.parent().segments);
  if (grid == null) return null;

  const [parentCol, parentOp] = parentOpSegment;
  return grid[parentCol]?.components[parentOp] ?? null;
};

/**
 * Find the parent component grid of an operation based on its location.
 *
 * Navigates via [`Location`](data/location.ts) so the addressing format is owned by exactly one
 * module.
 *
 * @param componentGrid The grid of components to search through.
 * @param location The location string of the operation.
 * @returns The parent grid of components, or `null` if the location is empty or addresses an op
 *   nested below a missing ancestor.
 */
const findParentArray = (
  componentGrid: ComponentGrid,
  location: string | null,
): ComponentGrid | null => {
  if (!location) return null;
  // Drop the last segment — it addresses the op itself; we want the grid that contains it, which is
  // keyed by its parent's segments.
  return _walkToGrid(componentGrid, Location.parse(location).parent().segments);
};

/**
 * Find an operation based on its location.
 *
 * Navigates via [`Location`](data/location.ts) so the addressing format is owned by exactly one
 * module.
 *
 * @param componentGrid The grid of components to search through.
 * @param location The location string of the operation.
 * @returns The operation at the given location, or `null` if the location is empty or addresses an
 *   op that does not exist.
 */
const findOperation = (
  componentGrid: ComponentGrid,
  location: string | null,
): Operation | null => {
  if (!location) return null;

  const last = Location.parse(location).last();
  if (last == null) return null;

  const operationParent = findParentArray(componentGrid, location);
  if (operationParent == null) return null;

  const [col, op] = last;
  return operationParent[col]?.components[op] ?? null;
};

/**
 * Set of qubit wires covered by EXTERNAL SIBLINGS of the op at `location` in that op's outer
 * column. "External sibling" = another op sharing the same column in the op's containing array
 * (i.e., NOT a descendant inside the op's own children grid).
 *
 * "Covered" follows the visual extent reported by `getWireRange`: the inclusive range of qubit rows
 * the sibling's body and any classical-control connector paint into. Classical-result endpoints
 * contribute the qubit row BELOW which they sit (e.g. a low endpoint at `q_c.r0` extends coverage
 * starting at `q_(c+1)`; a high endpoint at `q_c.r0` extends coverage through `q_c`).
 *
 * Use this to answer "what wires would directly collide if I widened this op's wire span?" — the
 * building block for the shift-extend dropzone filter, which composes this with
 * `getAncestorColumnSiblingWires` to walk the ancestor chain and accumulate every collision the
 * cascade would need to resolve.
 *
 * Returns an empty set when `location` is null, doesn't resolve, or addresses a top-level op with
 * no siblings (no overlap possible).
 *
 * @param componentGrid The grid the op lives in.
 * @param location The op's location string.
 * @returns The qubit wires covered by every external sibling in the op's outer column.
 */
const getOuterColumnSiblingWires = (
  componentGrid: ComponentGrid,
  location: string | null,
): Set<number> => {
  const blocked = new Set<number>();
  if (!location) return blocked;
  const last = Location.parse(location).last();
  if (last == null) return blocked;
  const [outerCol] = last;
  const parentArray = findParentArray(componentGrid, location);
  if (parentArray == null) return blocked;
  const column = parentArray[outerCol];
  if (column == null) return blocked;
  const selfOp = findOperation(componentGrid, location);
  for (const sibling of column.components) {
    if (sibling === selfOp) continue;
    const range = getWireRange(sibling);
    // Defensive: a register-less op (shouldn't exist) can't cover any wire.
    if (range == null) continue;
    const [minReg, maxReg] = range;
    // Classical-result row sits between q_owner and q_(owner+1). A LOW endpoint on a classical row
    // enters coverage at q_(owner+1); a HIGH endpoint on a classical row extends coverage through
    // q_owner. A quantum endpoint covers its own row at either end.
    const minQubit =
      minReg.result === undefined ? minReg.qubit : minReg.qubit + 1;
    const maxQubit = maxReg.qubit;
    for (let w = minQubit; w <= maxQubit; w++) {
      blocked.add(w);
    }
  }
  return blocked;
};

/**
 * Union of `getOuterColumnSiblingWires` across the op at `location` AND every ancestor of it, up to
 * (but not including) the root grid.
 *
 * Why walk the chain. The shift-extend gesture extends an immediately-enclosing group, but the
 * action layer's cascade (see [`refreshAncestorTargets`](actions/circuitActions.ts) +
 * [`_resolveOverlapAfterExtend`](actions/circuitActions.ts)) widens every ancestor whose span
 * doesn't already enclose the drop wire — and each widening may collide with siblings IN THAT
 * ANCESTOR'S column, not just the immediate parent's. Filtering only the immediate parent's
 * siblings under-blocks: a deeply nested source could be shift-dropped onto a wire owned by a
 * top-level sibling of an ancestor several levels up, leaving the cascade to silently cope (or, in
 * pathological cases, leave a visible overlap).
 *
 * Walks via location-string `parent()` rather than object identity so it can be called against the
 * live model without needing the ancestor chain pre-collected.
 *
 * Returns an empty set when `location` is null or doesn't resolve.
 *
 * @param componentGrid The grid the op lives in.
 * @param location The op's location string.
 * @returns The union of every ancestor's outer-column sibling wires, including the op's own outer
 *   column.
 */
const getAncestorColumnSiblingWires = (
  componentGrid: ComponentGrid,
  location: string | null,
): Set<number> => {
  const blocked = new Set<number>();
  if (!location) return blocked;
  let loc = Location.parse(location);
  while (!loc.isRoot && loc.last() != null) {
    const locStr = loc.toString();
    for (const wire of getOuterColumnSiblingWires(componentGrid, locStr)) {
      blocked.add(wire);
    }
    loc = loc.parent();
  }
  return blocked;
};

// Non-ASCII chars are fraught with danger. Copy/paste these when possible. Use the following regex
// in VS Code to find invalid unicode chars [^\x20-\x7e\u{03b8}-\u{03c8}\u{2020}\u{27e8}\u{27e9}]

const mathChars = {
  theta: "θ", // \u{03b8}
  pi: "π", // \u{03c0}
  psi: "ψ", // \u{03c8}
  dagger: "†", // \u{2020}
  langle: "⟨", // \u{27e8}
  rangle: "⟩", // \u{27e9}
};

/**
 * Given a click's SVG-space Y coordinate, the list of wire-Ys the clicked host element spans, and
 * the circuit's full wire-Y array, return the index (into `wireData`) of the wire whose Y is
 * **closest to the click**.
 *
 * Used by the selection controller to pick a per-click handle for multi-wire host elements (the
 * body of a group, SWAP, multi-qubit measurement, etc.), so the grabbed wire becomes the drag
 * handle rather than always pinning the gate's topmost wire to the drop wire.
 *
 * Behavior:
 *
 *   - `wireYs` is empty → return `-1` (no candidate). Caller should fall back to the static
 *     `data-wire` attribute.
 *   - `wireYs` has a single Y → return its `wireData` index directly. The click-Y is irrelevant for
 *     single-wire host elements (control dots, target circles, measurement crosses, ket boxes), and
 *     skipping the search avoids a pointless `getScreenCTM` call by the caller.
 *   - Multi-wire span → tie-break by smallest `|wireY - clickY|`, then by smaller `wireY`
 *     (deterministic on a tie). Returns `-1` if the winning Y isn't in `wireData` at all, which
 *     would indicate a renderer / editor wire-table mismatch.
 *
 * Clicks above the topmost wire or below the bottommost clamp to that endpoint, which is the
 * natural "closest" behavior — no special-case code needed.
 */
const pickClosestWireIndex = (
  clickSvgY: number,
  wireYs: ReadonlyArray<number>,
  wireData: ReadonlyArray<number>,
): number => {
  if (wireYs.length === 0) return -1;
  if (wireYs.length === 1) {
    return wireData.indexOf(wireYs[0]);
  }
  let bestY = wireYs[0];
  let bestDist = Math.abs(bestY - clickSvgY);
  for (let i = 1; i < wireYs.length; i++) {
    const y = wireYs[i];
    const dist = Math.abs(y - clickSvgY);
    if (dist < bestDist || (dist === bestDist && y < bestY)) {
      bestDist = dist;
      bestY = y;
    }
  }
  return wireData.indexOf(bestY);
};

export {
  deepEqual,
  getChildTargets,
  getGateLocationString,
  getMinMaxRegIdx,
  getOperationRegisters,
  getQuantumWireRange,
  getWireRange,
  findParentOperation,
  findParentArray,
  findOperation,
  getOuterColumnSiblingWires,
  getAncestorColumnSiblingWires,
  mathChars,
  pickClosestWireIndex,
};
