// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { ComponentGrid, Operation } from "../../data/circuit.js";
import { Register } from "../../data/register.js";
import { findOperation, getOperationRegisters } from "../../utils.js";

/*
 * `classicalRefs.ts` — classical-register producer/consumer
 * analysis for the Action layer.
 *
 * Measurements produce classical registers; classically-controlled
 * ops consume them. Keeping that graph consistent across moves and
 * deletes (document-order constraints, cascade-deletes, result-index
 * remaps) is the job of this module. Pure grid walks over the Data
 * layer — no DOM, no dependency on the other Action-layer modules.
 */

/**
 * Collect the set of classical-register IDs produced by any
 * measurement inside `op`'s subtree (including `op` itself). The
 * key is `"<qubit>:<result>"` because the consumer-side classical
 * control's `(qubit, result)` pair uniquely identifies the
 * classical register it reads.
 */
const collectInternalClassicalRegs = (
  op: Operation,
  set: Set<string>,
): void => {
  if (op.kind === "measurement") {
    for (const r of op.results) {
      if (r.result !== undefined) {
        set.add(`${r.qubit}:${r.result}`);
      }
    }
  }
  if (op.children) {
    for (const col of op.children) {
      for (const child of col.components) {
        collectInternalClassicalRegs(child, set);
      }
    }
  }
};

/**
 * Walk the entire grid (recursing into nested children) and build
 * a map from `"<qubit>:<result>"` to the **location string** of
 * the measurement operation that produces that classical
 * register. Locations use the same hierarchical format as the
 * rest of the editor (`"0,1"` for top level, `"0,1-2,3"` for
 * nested), which is exactly what
 * [`Location.inEarlierColumnThan`](../../data/location.ts) compares.
 *
 * Used by `collectExternalProducerLocations` (and indirectly by
 * the dropzone-filter pass and the `moveOperation` safety net)
 * to decide whether a candidate drop target preserves the
 * "producer column strictly earlier than consumer column"
 * invariant for every classical register a moved subtree
 * consumes.
 *
 * Multiple producers for the same key shouldn't happen in a
 * well-formed circuit, but if they do the **last** one wins —
 * that's the one document-order traversal would visit most
 * recently before any consumer.
 */
const _indexProducers = (grid: ComponentGrid): Map<string, string> => {
  const map = new Map<string, string>();
  const walk = (g: ComponentGrid, prefix: string): void => {
    g.forEach((col, ci) => {
      col.components.forEach((op, oi) => {
        const loc = prefix === "" ? `${ci},${oi}` : `${prefix}-${ci},${oi}`;
        if (op.kind === "measurement") {
          for (const r of op.results) {
            if (r.result !== undefined) {
              map.set(`${r.qubit}:${r.result}`, loc);
            }
          }
        }
        if (op.children) walk(op.children, loc);
      });
    });
  };
  walk(grid, "");
  return map;
};

/**
 * For the operation at `subtreeLocation`, return the locations of
 * every measurement that produces a classical register the
 * subtree consumes — but only when that producer lives **outside**
 * the subtree.
 *
 * Why "external only". Internal producers (M lives inside the
 * moved subtree) travel with the consumer when the subtree is
 * moved as a unit, so they impose no constraint on the drop
 * target. External producers stay put, so the consumer's new
 * position must still come after each of them in document order.
 *
 * Used by:
 *   - The dropzone-filter pass in
 *     [`DragController.onGateMouseDown`](../../editor/controllers/dragController.ts)
 *     to hide drop targets that would invert the
 *     producer-before-consumer ordering. (User-facing: invalid
 *     dropzones simply don't appear during the drag.)
 *   - The `moveOperation` safety net (refuses the move with
 *     `return null` if a producer ends up after the consumer)
 *     as a defense in depth in case a dropzone slips through.
 *
 * Returns an empty array if the op has no classical consumers,
 * if every consumer's producer is internal, or if the subtree
 * doesn't exist. Producers whose location can't be resolved are
 * silently skipped — we can't refuse moves we can't reason about.
 */
const collectExternalProducerLocations = (
  rootGrid: ComponentGrid,
  subtreeLocation: string,
): string[] => {
  const subtree = findOperation(rootGrid, subtreeLocation);
  if (subtree == null) return [];

  // Collect internal producers (their `"qubit:result"` keys) so
  // we can exclude them from the constraint check.
  const internalProducers = new Set<string>();
  collectInternalClassicalRegs(subtree, internalProducers);

  // Walk the subtree and collect every classical-ref's key
  // that is NOT in the internal set.
  const externalKeys = new Set<string>();
  const collectRefs = (op: Operation): void => {
    for (const r of getOperationRegisters(op)) {
      if (r.result !== undefined) {
        const key = `${r.qubit}:${r.result}`;
        if (!internalProducers.has(key)) externalKeys.add(key);
      }
    }
    if (op.children) {
      for (const col of op.children) {
        for (const c of col.components) collectRefs(c);
      }
    }
  };
  collectRefs(subtree);
  if (externalKeys.size === 0) return [];

  // Map every measurement in the grid to its location, then look
  // up each external key.
  const producers = _indexProducers(rootGrid);
  const locations: string[] = [];
  for (const key of externalKeys) {
    const loc = producers.get(key);
    if (loc != null) locations.push(loc);
  }
  return locations;
};

/**
 * For the measurement at `mLocation`, find every op anywhere in
 * the grid whose register-bearing fields hold a classical-ref
 * `(qubit, result)` that matches one of this M's `results`
 * entries — i.e. every downstream **consumer** of this M.
 *
 * Returned entries pair the consumer op (object reference, safe
 * to hand to `findAndRemoveOperations` as a predicate target)
 * with its current location string (needed for the column-order
 * partition the prompt-builder runs against the drop target).
 *
 * Locations use the same hierarchical format as the rest of the
 * editor (`"0,1"` top-level, `"0,1-2,3"` nested) — same shape
 * [`Location.parse`](../../data/location.ts) consumes.
 *
 * Walks into nested children: a classically-controlled gate
 * inside an unrelated group is still a consumer; deletion /
 * remap must reach it. The M op itself is excluded — an M is
 * never its own consumer.
 *
 * # Why `.controls` only (and not `.targets`)
 *
 * A "consumer" here is an op that **logically depends** on the
 * M's classical signal — i.e. its execution is gated by that
 * signal. For unitary ops, that's exactly an entry in `.controls`
 * whose `result` is defined. We deliberately do NOT inspect
 * `.targets`:
 *
 * - For a leaf unitary, `.targets` is purely a quantum-output
 *   site; classical refs never land there.
 * - For a group op, `.targets` is a derived cache that
 *   [`getChildTargets`](../../utils.ts) rebuilds by walking the
 *   subtree and dedup-merging every descendant's registers. If
 *   the group contains a classically-controlled child, the
 *   child's classical-ref **propagates up** into every ancestor
 *   group's `.targets` cache so the renderer can draw the
 *   group's visual span down to the classical wire.
 *
 * Treating those propagated `.targets` entries as consumption
 * would falsely flag every enclosing group as a consumer. The
 * cascade-delete in
 * [`removeMeasurementWithDependents`](circuitActions.ts) would then
 * wipe out the entire ancestor group — including unrelated sibling
 * children that don't depend on the M at all.
 *
 * A classically-controlled group is still correctly flagged:
 * its OWN `.controls` carries the classical ref (the eager
 * cache propagates from there outward, not the other way
 * around). And the group is a true logical consumer — when the
 * M is gone, the group's conditional execution is meaningless,
 * so cascade-removing it (and its dependent children) is right.
 *
 * Returns `[]` if the location doesn't resolve to a measurement,
 * the measurement has no classical results, or no op in the
 * grid references those results.
 */
const collectMeasurementConsumers = (
  rootGrid: ComponentGrid,
  mLocation: string,
): { op: Operation; location: string }[] => {
  const mOp = findOperation(rootGrid, mLocation);
  if (mOp == null || mOp.kind !== "measurement") return [];

  // Build the set of (qubit, result) keys this M produces.
  const producedKeys = new Set<string>();
  for (const r of mOp.results) {
    if (r.result !== undefined) {
      producedKeys.add(`${r.qubit}:${r.result}`);
    }
  }
  if (producedKeys.size === 0) return [];

  const consumers: { op: Operation; location: string }[] = [];
  const walk = (g: ComponentGrid, prefix: string): void => {
    g.forEach((col, ci) => {
      col.components.forEach((op, oi) => {
        const loc = prefix === "" ? `${ci},${oi}` : `${prefix}-${ci},${oi}`;
        // Skip the M itself — but still recurse into its children
        // (defensive: an M with children isn't a real shape today,
        // but the walk shouldn't depend on that).
        if (op !== mOp) {
          // Logical consumption lives in `.controls` only — see
          // the long-form rationale in the doc comment above.
          const controls = op.kind === "unitary" ? op.controls : undefined;
          if (controls) {
            for (const reg of controls) {
              if (
                reg.result !== undefined &&
                producedKeys.has(`${reg.qubit}:${reg.result}`)
              ) {
                consumers.push({ op, location: loc });
                break;
              }
            }
          }
        }
        if (op.children) walk(op.children, loc);
      });
    });
  };
  walk(rootGrid, "");
  return consumers;
};

/**
 * Walk the grid and remap every classical-ref entry's
 * `(qubit, result)` pair according to `keyRemap`. Visits both
 * the op's own register-bearing fields AND the cached
 * `.targets` / `.controls` on group ops (which hold their own
 * `Register` objects, independent of descendant ops — see
 * [`_dedupRegistersByIdentity`](derivedTargets.ts)).
 *
 * Only classical refs (`result !== undefined`) are touched;
 * quantum refs are left alone.
 */
const applyClassicalRefRemap = (
  grid: ComponentGrid,
  keyRemap: Map<string, string>,
): void => {
  const remapRegister = (reg: Register): void => {
    if (reg.result === undefined) return;
    const preKey = `${reg.qubit}:${reg.result}`;
    const postKey = keyRemap.get(preKey);
    if (postKey == null) return;
    const colonIdx = postKey.indexOf(":");
    reg.qubit = parseInt(postKey.substring(0, colonIdx), 10);
    reg.result = parseInt(postKey.substring(colonIdx + 1), 10);
  };
  const walk = (g: ComponentGrid): void => {
    for (const col of g) {
      for (const op of col.components) {
        // Walk the op's CONSUMER-side register fields only. A
        // measurement's `.results` is the PRODUCER side: its
        // (qubit, result) keys were authoritatively assigned by
        // the `updateMeasurementLines` sweep that ran inside
        // `moveOperation`. Feeding those producer values back
        // through the consumer remap would double-remap them —
        // any M whose freshly-assigned result index happened to
        // match another M's pre-move key would get rewritten a
        // second time, collapsing two Ms onto the same key and
        // orphaning the consumer that was supposed to reference
        // the original M. So for measurements we only visit
        // `.qubits` (no-op for the remap since they have
        // `result === undefined`, but kept for symmetry); for
        // unitaries and kets we visit all registers, because
        // their `.targets` and `.controls` are all references
        // to producers elsewhere — including group ops whose
        // eager `.targets` cache holds classical refs aliased
        // from descendant consumers.
        if (op.kind === "measurement") {
          for (const reg of op.qubits) remapRegister(reg);
        } else {
          for (const reg of getOperationRegisters(op)) {
            remapRegister(reg);
          }
        }
        if (op.children) walk(op.children);
      }
    }
  };
  walk(grid);
};

/**
 * Walk the grid for an op matching `target` by object identity
 * and return its hierarchical location string, or `null` if not
 * found. Used by callers (e.g. `removeMeasurementWithDependents`)
 * that capture an op reference BEFORE a mutation that may shift
 * its location, then need a fresh location string AFTER the
 * mutation.
 */
const findLocationByRef = (
  grid: ComponentGrid,
  target: Operation,
): string | null => {
  const walk = (g: ComponentGrid, prefix: string): string | null => {
    for (let ci = 0; ci < g.length; ci++) {
      for (let oi = 0; oi < g[ci].components.length; oi++) {
        const op = g[ci].components[oi];
        const loc = prefix === "" ? `${ci},${oi}` : `${prefix}-${ci},${oi}`;
        if (op === target) return loc;
        if (op.children) {
          const r = walk(op.children, loc);
          if (r != null) return r;
        }
      }
    }
    return null;
  };
  return walk(grid, "");
};

export {
  applyClassicalRefRemap,
  collectInternalClassicalRegs,
  findLocationByRef,
  collectExternalProducerLocations,
  collectMeasurementConsumers,
};
