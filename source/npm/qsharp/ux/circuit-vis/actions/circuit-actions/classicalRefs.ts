// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { ComponentGrid, Operation } from "../../data/circuit.js";
import { CircuitModel } from "../../data/circuitModel.js";
import { Register } from "../../data/register.js";
import { findOperation, getOperationRegisters } from "../../utils.js";

/*
 * `classicalRefs.ts` — classical-register producer/consumer analysis for the Action layer.
 *
 * Measurements produce classical registers; classically-controlled ops consume them. This module
 * keeps that graph consistent across moves and deletes (document-order constraints,
 * cascade-deletes, result-index remaps). Pure grid walks over the Data layer — no DOM.
 */

/**
 * Collect the classical-register IDs produced by any measurement in `op`'s subtree (including
 * `op`). Keyed `"<qubit>:<result>"`, the pair a consumer's classical control reads.
 */
const collectInternalClassicalRegs = (op: Operation): Set<string> => {
  const set = new Set<string>();
  const walk = (o: Operation): void => {
    if (o.kind === "measurement") {
      for (const r of o.results) {
        if (r.result !== undefined) {
          set.add(`${r.qubit}:${r.result}`);
        }
      }
    }
    if (o.children) {
      for (const col of o.children) {
        for (const child of col.components) {
          walk(child);
        }
      }
    }
  };
  walk(op);
  return set;
};

/**
 * Map every classical register to the location string of the measurement that produces it
 * (`"<qubit>:<result>"` → location). Locations use the editor's hierarchical format (`"0,1"`,
 * `"0,1-2,3"`), as compared by [`Location.inEarlierColumnThan`](../../data/location.ts).
 *
 * Used by `collectExternalProducerLocations` (and indirectly the dropzone filter and
 * `moveOperation` safety net) to enforce "producer column strictly earlier than consumer." If a key
 * has multiple producers (shouldn't happen in a well-formed circuit), the last one wins.
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
 * For the operation at `subtreeLocation`, return the locations of every measurement that produces a
 * classical register the subtree consumes — but only producers living OUTSIDE the subtree. Internal
 * producers travel with the consumer when the subtree moves as a unit, so they impose no
 * drop-target constraint; external producers stay put, so the consumer's new position must come
 * after them.
 *
 * Used by:
 *   - The dropzone-filter pass in
 *     [`DragController.onGateMouseDown`](../../editor/controllers/dragController.ts) to hide drop
 *     targets that would invert producer-before-consumer.
 *   - The `moveOperation` safety net (returns `null` if a producer ends up after the consumer) as
 *     defense in depth.
 *
 * Returns `[]` if the op has no external classical consumers or the subtree doesn't exist.
 * Producers whose location can't be resolved are skipped.
 */
const collectExternalProducerLocations = (
  rootGrid: ComponentGrid,
  subtreeLocation: string,
): string[] => {
  const subtree = findOperation(rootGrid, subtreeLocation);
  if (subtree == null) return [];

  // Collect internal producers (their `"qubit:result"` keys) so we can exclude them from the
  // constraint check.
  const internalProducers = collectInternalClassicalRegs(subtree);

  // Walk the subtree and collect every classical-ref's key that is NOT in the internal set.
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

  // Map every measurement in the grid to its location, then look up each external key.
  const producers = _indexProducers(rootGrid);
  const locations: string[] = [];
  for (const key of externalKeys) {
    const loc = producers.get(key);
    if (loc != null) locations.push(loc);
  }
  return locations;
};

/**
 * For the measurement at `mLocation`, find every downstream consumer: any op whose register fields
 * hold a classical-ref `(qubit, result)` matching one of this M's `results`. Returned entries pair
 * the consumer op (object reference) with its location string. Walks into nested children; the M op
 * itself is excluded.
 *
 * Only `.controls` count as consumption (not `.targets`): a consumer is an op whose execution is
 * GATED by the M's signal, which for unitaries is a `.controls` entry with `result` defined. A
 * group's `.targets` is a derived cache that propagates a classically-controlled child's ref up
 * into every ancestor; treating those as consumption would falsely flag every enclosing group and
 * the cascade-delete would wipe out unrelated siblings. A classically-controlled group is still
 * flagged correctly via its own `.controls`.
 *
 * Returns `[]` if the location isn't a measurement, the M has no classical results, or nothing
 * references them.
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
        // Skip the M itself, but still recurse into its children.
        if (op !== mOp) {
          // Logical consumption lives in `.controls` only.
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
 * Walk the grid for an op matching `target` by object identity and return its hierarchical location
 * string, or `null` if not found. Used by callers (e.g. `removeMeasurementWithDependents`) that
 * capture an op reference BEFORE a mutation that may shift its location, then need a fresh location
 * string AFTER the mutation.
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

/**
 * Tag every classical register on the move's affected wires with a stable identity (a unique
 * negative token) so producer→consumer links survive the wire-shift and result-renumber. Call BEFORE
 * `moveY`/`_doShift`, then `decodeClassicalResultTokens` afterward to restore real indices.
 *
 * The token lives in the `result` field (a negative sentinel; real indices are ≥ 0) and rides
 * through the move untouched, since `_doShift` only changes `.qubit` and the JSON clone copies the
 * number. Affected wires = every measurement wire the moved subtree touches plus that wire + `delta`
 * (its landing); registers elsewhere keep their real indices.
 *
 * `originalOp` is the pre-move op still in `grid` (skipped, about to be removed); `cloneOp` is its
 * replacement (not yet in `grid`, walked separately). Both share one key→token map so a producer and
 * all its consumers — inside the clone or elsewhere — resolve to the same token.
 *
 * Returns the set of affected wires (source + landing), which `decodeClassicalResultTokens` reuses
 * to refresh each wire's `numResults` counter — no separate measurement-wire sweep needed.
 */
const encodeClassicalResultTokens = (
  grid: ComponentGrid,
  originalOp: Operation,
  cloneOp: Operation,
  delta: number,
): Set<number> => {
  // Affected wires: the moved subtree's measurement wires (pre-shift, read off the clone) and their
  // post-shift landings.
  const affectedWires = new Set<number>();
  const collectWires = (o: Operation): void => {
    if (o.kind === "measurement") {
      for (const q of o.qubits) {
        affectedWires.add(q.qubit);
        affectedWires.add(q.qubit + delta);
      }
    }
    if (o.children) {
      for (const col of o.children) {
        for (const c of col.components) collectWires(c);
      }
    }
  };
  collectWires(cloneOp);
  if (affectedWires.size === 0) return affectedWires;

  // Walk `grid` (skipping the doomed `originalOp` subtree) and `cloneOp`, applying `visit` to each op.
  const walkAll = (visit: (op: Operation) => void): void => {
    const walk = (g: ComponentGrid): void => {
      for (const col of g) {
        for (const op of col.components) {
          if (op === originalOp) continue; // about to be removed; its clone is walked below.
          visit(op);
          if (op.children) walk(op.children);
        }
      }
    };
    walk(grid);
    const walkClone = (o: Operation): void => {
      visit(o);
      if (o.children) {
        for (const col of o.children) {
          for (const c of col.components) walkClone(c);
        }
      }
    };
    walkClone(cloneOp);
  };

  const keyToToken = new Map<string, number>();
  let nextToken = -1;

  // Pass 1: tokenize producers (measurement `.results`) on affected wires. Record each under its
  // original `(qubit, result)` key so consumers can find it in pass 2.
  walkAll((op) => {
    if (op.kind !== "measurement") return;
    for (const r of op.results) {
      if (r.result === undefined || r.result < 0) continue;
      if (!affectedWires.has(r.qubit)) continue;
      const key = `${r.qubit}:${r.result}`;
      let token = keyToToken.get(key);
      if (token === undefined) {
        token = nextToken--;
        keyToToken.set(key, token);
      }
      r.result = token;
    }
  });

  // Pass 2: point every consumer of a tokenized producer at the same token. A measurement's own
  // `.results` are producers (already tokenized in pass 1); its `.qubits` are quantum. Every other
  // op's classical refs (own `.controls`/`.targets` plus derived group caches) are consumers.
  walkAll((op) => {
    const regs =
      op.kind === "measurement" ? op.qubits : getOperationRegisters(op);
    for (const reg of regs) {
      if (reg.result === undefined || reg.result < 0) continue;
      const token = keyToToken.get(`${reg.qubit}:${reg.result}`);
      if (token !== undefined) reg.result = token;
    }
  });

  return affectedWires;
};

/**
 * Renumber the classical producers on a set of wires and carry their downstream consumers along, in
 * two passes over the grid. `keyOf` maps a register to the value that bridges a producer to its
 * consumers.
 *
 * Pass 1 walks every measurement on a wire in `wires` in document order, keys each producer via
 * `keyOf` (captured BEFORE its index is overwritten), assigns it the next contiguous index on its
 * wire, and records key → final `(qubit, result)`. It then refreshes `model.qubits[wire].numResults`
 * for every wire in `wires`. Pass 2 walks every consumer, keys it via the same `keyOf`, and repoints
 * any whose key was recorded in pass 1. A measurement's own `.results` are producers (renumbered in
 * pass 1); its `.qubits` are quantum; every other op's classical refs are consumers.
 */
const reconcileClassicalResults = <K>(
  model: CircuitModel,
  wires: Set<number>,
  keyOf: (reg: Register) => K,
): void => {
  if (wires.size === 0) return;

  const grid = model.componentGrid;
  const keyToNew = new Map<K, Register>();
  const perWireCount = new Map<number, number>();

  // Pass 1: renumber producers on the affected wires in document order.
  const renumber = (g: ComponentGrid): void => {
    for (const col of g) {
      for (const op of col.components) {
        if (op.kind === "measurement") {
          for (const r of op.results) {
            if (r.result === undefined || !wires.has(r.qubit)) continue;
            const key = keyOf(r);
            const idx = perWireCount.get(r.qubit) ?? 0;
            perWireCount.set(r.qubit, idx + 1);
            r.result = idx;
            keyToNew.set(key, { qubit: r.qubit, result: idx });
          }
        }
        if (op.children) renumber(op.children);
      }
    }
  };
  renumber(grid);

  // Refresh per-wire `numResults` for every affected wire. A wire left with no measurements resets
  // to `undefined`.
  for (const wire of wires) {
    if (wire >= 0 && wire < model.qubits.length) {
      const count = perWireCount.get(wire) ?? 0;
      model.qubits[wire].numResults = count > 0 ? count : undefined;
    }
  }

  if (keyToNew.size === 0) return;

  // Pass 2: repoint every consumer whose key was remapped in pass 1.
  const repoint = (g: ComponentGrid): void => {
    for (const col of g) {
      for (const op of col.components) {
        const regs =
          op.kind === "measurement" ? op.qubits : getOperationRegisters(op);
        for (const reg of regs) {
          if (reg.result === undefined) continue;
          const dest = keyToNew.get(keyOf(reg));
          if (dest !== undefined) {
            reg.qubit = dest.qubit;
            reg.result = dest.result;
          }
        }
        if (op.children) repoint(op.children);
      }
    }
  };
  repoint(grid);
};

/**
 * Undo `encodeClassicalResultTokens`: assign real, contiguous result indices to the tokenized
 * producers and repoint their tokenized consumers to the producers' final `(qubit, result)`. Call
 * AFTER the physical move + span resolution, when document order is settled.
 *
 * Keys by the token integer stamped on both producer and consumer by encode — the producer's
 * original `(qubit, result)` is destroyed by the shift, so only the shift-immune token bridges them.
 * Also refreshes `numResults` for every wire in `affectedWires` (the move's source and landing
 * wires), which fully replaces the tail-end `updateMeasurementLines` sweep the move path used to run.
 */
const decodeClassicalResultTokens = (
  model: CircuitModel,
  affectedWires: Set<number>,
): void => {
  reconcileClassicalResults(model, affectedWires, (reg) => reg.result!);
};

/**
 * Resequence classical result indices on a set of wires after a STRUCTURAL edit (plain add / remove
 * of a measurement) and carry every downstream consumer along. The add/remove counterpart to the
 * move path's `decodeClassicalResultTokens`; it needs no encode step because a structural edit never
 * relocates a producer off its wire, so a producer's pre-edit `(qubit, result)` is a stable, unique
 * key its consumers can be matched on directly. A freshly-added measurement carries a sentinel index
 * (`-1`) that shares no key with any real producer, so its (consumer-less) entry never collides.
 */
const resequenceClassicalResults = (
  model: CircuitModel,
  wires: Set<number>,
): void => {
  reconcileClassicalResults(
    model,
    wires,
    (reg) => `${reg.qubit}:${reg.result}`,
  );
};

export {
  encodeClassicalResultTokens,
  decodeClassicalResultTokens,
  resequenceClassicalResults,
  collectInternalClassicalRegs,
  findLocationByRef,
  collectExternalProducerLocations,
  collectMeasurementConsumers,
};
