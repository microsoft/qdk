// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { Column, ComponentGrid, Operation, Unitary } from "../data/circuit.js";
import { CircuitModel } from "../data/circuitModel.js";
import { Location } from "../data/location.js";
import { Register } from "../data/register.js";
import {
  findOperation,
  findParentArray,
  getOperationRegisters,
  getQuantumWireRange,
} from "../utils.js";

/*
 * `circuitActions.ts` — the **Action layer** in the circuit editor's
 * three-layer architecture (Data / Action / View).
 *
 * Each exported function takes a `CircuitModel` (Data layer) as its
 * first argument and mutates it in place. **No DOM. No interaction
 * state. No rendering.** Functions return either the new/affected
 * `Operation` (when the caller needs a handle to it) or a `boolean`
 * status flag, depending on what the calling UI code needs.
 *
 * Because Actions are pure data mutations, they can be exercised
 * directly against a freshly-constructed `CircuitModel` with no
 * JSDOM and no `CircuitEvents` stub.
 */

/**
 * Move an operation in the circuit.
 *
 * After the move settles, both the source-side and dest-side
 * ancestor chains are walked innermost-out by
 * [`refreshAncestorTargets`](#) and each still-attached
 * ancestor's derived `.targets`/`.results` is re-built from its
 * (post-move) children. The walks cascade upward — refreshing
 * each rung whose pre-existing span no longer encloses the
 * deeper subtree's new span — and stop at the first rung that
 * already does. This keeps the invariant "an ancestor's
 * `.targets` is the union of its descendants' wires (plus any
 * classical-control refs the group itself carries)" intact for
 * arbitrary drop locations.
 *
 * The target location string carries the authoritative intent of
 * which group the moved op lands in; the cascade is correctness,
 * not opt-in policy.
 *
 * @param model The circuit model to mutate.
 * @param sourceLocation The location string of the source operation.
 * @param targetLocation The location string of the target position.
 * @param sourceWire The wire index of the source operation.
 * @param targetWire The wire index to move the operation to.
 * @param movingControl Whether the operation is being moved as a control.
 * @param insertNewColumn Whether to insert a new column when adding the operation.
 * @returns The moved operation or null if the move was unsuccessful.
 */
const moveOperation = (
  model: CircuitModel,
  sourceLocation: string,
  targetLocation: string,
  sourceWire: number,
  targetWire: number,
  movingControl: boolean,
  insertNewColumn: boolean = false,
): Operation | null => {
  const originalOperation = findOperation(model.componentGrid, sourceLocation);

  if (originalOperation == null) return null;

  // Resolve source-side parent references BEFORE any mutation.
  //
  // `_moveX` below may splice a fresh column into a grid that lies
  // on the source's path — e.g. moving a child out of a group to a
  // new top-level column at index 0 shifts the group's top-level
  // column index from N to N+1, which in turn invalidates the
  // source's hierarchical location string ("N,0-..." no longer
  // names the same op). A location-string lookup after the mutation
  // would walk the wrong path and either return null (leaving the
  // original op in place as a phantom duplicate) or return some
  // unrelated grid (corrupting the removal).
  //
  // Capturing the array reference itself sidesteps the issue: the
  // reference stays valid as its contents shift around it.
  const sourceOperationParent = findParentArray(
    model.componentGrid,
    sourceLocation,
  );
  if (sourceOperationParent == null) return null;

  // Capture the full ancestor chain BEFORE any mutation so the
  // empty-group cleanup at the tail of this function still has
  // valid references even after `_moveX` splices columns around.
  // (See `_collectAncestorChain` for why this has to be pre-move.)
  const ancestorChain = _collectAncestorChain(model, sourceLocation);

  // Capture the destination's ancestor chain pre-move too, for the
  // dest-side cascade refresh below. Same reasoning as the source
  // chain: post-move, `targetLocation` may name a different op
  // (column splices, empty-prune cascades) and a string-based walk
  // would land on the wrong tree. Object references stay valid.
  // Empty array when there is no parent group (top-level drop).
  const destAncestorChain: AncestorRung[] = _collectDestAncestorChain(
    model,
    targetLocation,
  );

  // Safety net: refuse the move if it would place the source
  // before one of its external classical-register producers in
  // document order. The dropzone-filter pass in
  // [`DragController`](../editor/controllers/dragController.ts)
  // hides invalid dropzones at drag-start so the user can't even
  // initiate such a drop; this guard catches any path that bypasses
  // that filter (programmatic moves, future code paths, race
  // conditions). Same `return null` no-op contract as the
  // negative-wire refusal below — `dragController` treats null as
  // "drop rejected, no re-render".
  //
  // Comparison is on PRE-mutation locations: if a producer's
  // current column is not strictly earlier than the candidate
  // `targetLocation`'s column (with ancestor groups projecting
  // their column onto everything they contain), the move would
  // place the consumer at-or-before the producer's time-step.
  // See [`Location.inEarlierColumnThan`](../data/location.ts) for
  // the comparison rules — note that two ops in the same column
  // are simultaneous, not predecessor/successor, so a consumer
  // "promoted" to a sibling op-position of the producer's outer
  // group is still in the producer's column and gets refused.
  const externalProducerLocs = collectExternalProducerLocations(
    model.componentGrid,
    sourceLocation,
  );
  if (externalProducerLocs.length > 0) {
    const targetLoc = Location.parse(targetLocation);
    for (const pLocStr of externalProducerLocs) {
      const pLoc = Location.parse(pLocStr);
      if (!pLoc.inEarlierColumnThan(targetLoc)) return null;
    }
  }

  // Create a deep copy of the source operation
  const newSourceOperation: Operation = JSON.parse(
    JSON.stringify(originalOperation),
  );

  // Capture pre-move measurement wires from the live source. Used
  // after the move to refresh per-wire `numResults` counters for
  // any wire whose measurement set may have changed (see the
  // `_updateMeasurementLines` sweep at the tail of this function).
  const affectedMeasurementWires = new Set<number>();
  _collectMeasurementWires(originalOperation, affectedMeasurementWires);

  // Grow the model to accommodate the highest wire the post-move
  // op will land on. For a single-leg move this is `targetWire`.
  // For a group / multi-target move (the unit-shift path inside
  // `_moveY`) every register shifts by `targetWire - sourceWire`,
  // so the high wire moves to `maxOrigWire + delta` — which can
  // be well above `targetWire` and must exist before `_moveX`
  // tries to file the op into the grid.
  //
  // We also refuse the move outright if the unit-shift would push
  // any wire below 0. The model has no concept of "negative wires"
  // and no machinery to insert wires above wire 0; silently letting
  // it happen leaves the subtree with `qubit: -N` register refs and
  // the next render throws (or, after `removeTrailingUnusedQubits`
  // trims the model, throws with a misleading "Classical register
  // ID X invalid for qubit ID Y with 0 classical register(s)" when
  // the trim cuts the wire a classical register lived on). The
  // user-visible effect is the drop silently no-ops; the dragController
  // sees a `null` return and skips the re-render.
  if (_moveAsUnit(newSourceOperation, movingControl)) {
    const delta = targetWire - sourceWire;
    const [minOrigWire, maxOrigWire] =
      _getSubtreeMinMaxWire(newSourceOperation);
    if (minOrigWire >= 0 && minOrigWire + delta < 0) {
      return null;
    }
    model.ensureQubitCount(Math.max(targetWire, maxOrigWire + delta));
  } else {
    model.ensureQubitCount(targetWire);
  }

  // Update operation's targets and controls
  _moveY(newSourceOperation, sourceWire, targetWire, movingControl);

  // Capture POST-shift measurement wires too, so the refresh sweep
  // covers both the wires the measurements just left AND the wires
  // they just landed on.
  _collectMeasurementWires(newSourceOperation, affectedMeasurementWires);

  // Move horizontally
  _moveX(
    model,
    newSourceOperation,
    originalOperation,
    targetLocation,
    insertNewColumn,
  );

  _removeOp(model, originalOperation, sourceOperationParent);

  // Source-side cleanup: prune any ancestor groups whose
  // children just collapsed to empty (cascades upward) and then
  // refresh the surviving ancestors' derived `.targets`. The two
  // sweeps used to be interleaved inside `_pruneEmptyAncestors`
  // (with a separate inline refresh on the source's direct
  // parent ahead of it); D7 folded both into the single
  // `refreshAncestorTargets` utility, which handles the
  // narrowing case (a child went away) symmetrically with the
  // widening case on the dest side below.
  //
  // Order matters: prune first, refresh second. `_isOperationEmpty`
  // reads `children`, not `.targets`, so refreshing a rung that's
  // about to be deleted is wasted work. Pruning first also lets
  // the refresh walk skip already-detached rungs via the same
  // `stillAttached` check that the dest side uses.
  //
  // Runs BEFORE the measurement-line sweep and
  // `removeTrailingUnusedQubits` so those passes see the already-
  // cleaned tree.
  const survivedSourceChain = _pruneEmptyAncestors(ancestorChain);
  refreshAncestorTargets(survivedSourceChain);

  // Dest-side cascading refresh. Each ancestor of the drop target
  // whose `.targets` no longer encloses its child's (possibly
  // widened) wire span gets re-derived; the per-rung
  // `onAfterRefresh` hook resolves any sibling-column overlap the
  // widening just introduced (split-and-shift, mirroring the
  // `commitAddControl` pattern).
  //
  // Always-on because the target location string is authoritative:
  // if the user dropped the source at a location inside group G,
  // then G IS the source's new parent, and G's `.targets` MUST
  // reflect that. No-op when the drop is top-level (empty chain)
  // or when every dest ancestor was pruned by the source-side
  // sweep above — see `refreshAncestorTargets`'s `stillAttached`
  // check.
  refreshAncestorTargets(destAncestorChain, {
    onAfterRefresh: ({ op, containingArray }) =>
      _resolveOverlapAfterExtend(op, containingArray),
  });

  // Refresh per-wire `numResults` counters for every wire that
  // may have gained or lost a measurement. `_addOp` / `_removeOp`
  // only fire this for TOP-LEVEL measurements; when a measurement
  // crosses wires inside a moved group, this sweep is the only
  // thing that keeps `qubits[wire].numResults` in step with the
  // measurements actually present on that wire. Stale numResults
  // is exactly what causes the renderer to throw
  // "Classical register ID N invalid for qubit ID M with 0
  // classical register(s)" the next paint.
  for (const wire of affectedMeasurementWires) {
    if (wire >= 0 && wire < model.qubits.length) {
      _updateMeasurementLines(model, wire);
    }
  }

  model.removeTrailingUnusedQubits();

  return newSourceOperation;
};

/**
 * Move an operation horizontally.
 */
const _moveX = (
  model: CircuitModel,
  sourceOperation: Operation,
  originalOperation: Operation,
  targetLocation: string,
  insertNewColumn: boolean = false,
) => {
  const targetOperationParent = findParentArray(
    model.componentGrid,
    targetLocation,
  );

  const targetLastIndex = Location.parse(targetLocation).last();

  if (targetOperationParent == null || targetLastIndex == null) return;

  // Insert sourceOperation to target last index
  _addOp(
    model,
    sourceOperation,
    targetOperationParent,
    targetLastIndex,
    insertNewColumn,
    originalOperation,
  );
};

/**
 * Should we move `op` as a single rigid unit (shift every register
 * by the same delta), or as a single leg (rewire just the one
 * register the user grabbed)?
 *
 * "As a unit" applies when:
 *   - `op` is a group (has `children`). The user grabbed the box
 *     and expects the contained children to come along; rewriting
 *     the group's `.targets` to a single wire would just relocate
 *     the box and leave the children on their original wires.
 *   - `op` has more than one target/qubit in the relevant axis
 *     (e.g. SWAP, multi-qubit measurement). Single-leg behavior
 *     would collapse `targets`/`qubits` down to one wire and
 *     destroy the gate.
 *
 * Single-leg behavior is preserved for the ordinary controlled-gate
 * cases (one target + N controls) so the user can drag the target
 * or any one control independently — that's the established
 * "rewire one leg of a CNOT" interaction.
 */
const _moveAsUnit = (op: Operation, movingControl: boolean): boolean => {
  if (op.children != null) return true;
  if (movingControl) return false;
  switch (op.kind) {
    case "unitary":
    case "ket":
      return op.targets.length > 1;
    case "measurement":
      return op.qubits.length > 1;
  }
};

/**
 * Shift every wire-axis register of `op` — and, recursively, every
 * wire-axis register of every child op — by `delta`. Used when
 * moving a multi-wire op (group or multi-target/multi-qubit gate)
 * as a rigid unit, so the whole gate keeps its shape on the new
 * wires.
 *
 * Classical controls (registers whose `result` field is set; they
 * point at an external classical register identified by the
 * `(qubit, result)` tuple of the wire that owns it) need careful
 * handling. The right question is **not** "is this register
 * classical?" but **"is the thing it references also moving?"**:
 *
 *   - If the producing measurement lives **inside** the moved
 *     subtree, the producer is shifting by the same `delta`, so
 *     the consumer must shift too to stay aligned with it.
 *   - If the producing measurement lives **outside** the moved
 *     subtree, the classical register it produced stays put, so
 *     the consumer must stay anchored to its current wire.
 *
 * To distinguish, we first walk the subtree and collect the set
 * of `(qubit, result)` tuples that measurements **inside** it
 * produce. Then for each classical control we encounter while
 * shifting, we look it up in that set: present → shift, absent
 * → anchor.
 */
const _shiftAllRegisters = (op: Operation, delta: number): void => {
  if (delta === 0) return;
  const internalProducers = new Set<string>();
  _collectInternalClassicalRegs(op, internalProducers);
  _doShift(op, delta, internalProducers);
};

/**
 * Collect the set of classical-register IDs produced by any
 * measurement inside `op`'s subtree (including `op` itself). The
 * key is `"<qubit>:<result>"` because the consumer-side classical
 * control's `(qubit, result)` pair uniquely identifies the
 * classical register it reads.
 */
const _collectInternalClassicalRegs = (
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
        _collectInternalClassicalRegs(child, set);
      }
    }
  }
};

/**
 * The actual recursive shift. See `_shiftAllRegisters` for the
 * classical-control rationale.
 *
 * The rule is uniform across every register on every op:
 *   - Quantum register (`result === undefined`) → always shift.
 *     It identifies a wire the op acts on, and that wire is
 *     moving with us.
 *   - Classical-register reference (`result !== undefined`) →
 *     shift iff the producing measurement lives inside the moved
 *     subtree (i.e. its `(qubit, result)` tuple is in
 *     `internalProducers`); anchor otherwise.
 *
 * This applies to **all** register-bearing fields, not just
 * `controls`. Notably, classically-conditional unitaries record
 * their classical-register dependencies in BOTH `controls` AND
 * `targets` (the `targets` entries are visual extent claims that
 * draw the line from the gate down to the classical register
 * box). A producer-external classical entry in `targets` that we
 * naively shifted would re-point the visual extent at a wire with
 * no classical registers — and the renderer throws
 * "Classical register ID X invalid for qubit ID Y with 0 classical
 * register(s)" trying to address it.
 */
const _doShift = (
  op: Operation,
  delta: number,
  internalProducers: Set<string>,
): void => {
  for (const reg of getOperationRegisters(op)) {
    if (reg.result === undefined) {
      reg.qubit += delta;
    } else if (internalProducers.has(`${reg.qubit}:${reg.result}`)) {
      reg.qubit += delta;
    }
    // else: external classical-register reference → anchor in place.
  }
  if (op.children) {
    for (const col of op.children) {
      for (const child of col.components) {
        _doShift(child, delta, internalProducers);
      }
    }
  }
};

/**
 * Collect the set of wires that have at least one measurement
 * anywhere in `op`'s subtree. Used to know which wires' per-wire
 * `numResults` counters need to be refreshed after a move.
 */
const _collectMeasurementWires = (op: Operation, set: Set<number>): void => {
  if (op.kind === "measurement") {
    for (const q of op.qubits) set.add(q.qubit);
  }
  if (op.children) {
    for (const col of op.children) {
      for (const child of col.components) {
        _collectMeasurementWires(child, set);
      }
    }
  }
};

/**
 * Walk `op` and every descendant to find the lowest and highest
 * **quantum** wire (i.e. registers whose `result` field is
 * undefined; classical-register entries are skipped because they
 * reference a producer's wire, not a wire `op` acts on).
 *
 * Used by `moveOperation` to refuse a unit-shift that would push
 * any wire below 0, and to know how far to grow the model on the
 * high side. Walking the subtree (not just the top-level op) is
 * essential for groups whose root `.targets` is just a derived
 * extent claim and may miss wires that only appear in deeply
 * nested children.
 *
 * Returns `[-1, -1]` if the subtree references no quantum wires.
 */
const _getSubtreeMinMaxWire = (op: Operation): [number, number] => {
  let min = Number.POSITIVE_INFINITY;
  let max = -1;
  const walk = (o: Operation): void => {
    for (const r of getOperationRegisters(o)) {
      if (r.result === undefined) {
        if (r.qubit < min) min = r.qubit;
        if (r.qubit > max) max = r.qubit;
      }
    }
    if (o.children) {
      for (const col of o.children) {
        for (const c of col.components) walk(c);
      }
    }
  };
  walk(op);
  return [Number.isFinite(min) ? min : -1, max];
};

/**
 * Walk the entire grid (recursing into nested children) and build
 * a map from `"<qubit>:<result>"` to the **location string** of
 * the measurement operation that produces that classical
 * register. Locations use the same hierarchical format as the
 * rest of the editor (`"0,1"` for top level, `"0,1-2,3"` for
 * nested), which is exactly what
 * [`Location.inEarlierColumnThan`](../data/location.ts) compares.
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
 *     [`DragController.onGateMouseDown`](../editor/controllers/dragController.ts)
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
  _collectInternalClassicalRegs(subtree, internalProducers);

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
 * Type alias for one rung of the source op's ancestor chain
 * captured for the empty-group cleanup pass. Each entry pairs an
 * ancestor operation with the array reference that contains it,
 * captured BEFORE the move mutates anything. Both references stay
 * valid through `_moveX` / `_removeOp` because we hold the array
 * itself, not a location string into it.
 */
type AncestorRung = { op: Operation; containingArray: ComponentGrid };

/**
 * Collect the source op's ancestor chain, innermost-first, up to
 * (but not including) the root grid. Used by the empty-group
 * cleanup pass in `moveOperation`.
 *
 * Walks location strings during capture (before any mutation), but
 * the captured references are object references — they remain
 * valid even if `_moveX` later invalidates location strings by
 * splicing columns elsewhere in the tree.
 */
const _collectAncestorChain = (
  model: CircuitModel,
  sourceLocation: string,
): AncestorRung[] => {
  const chain: AncestorRung[] = [];
  // Source's PARENT is the innermost ancestor. We never include the
  // source op itself; that's already removed by `_removeOp`.
  let loc = Location.parse(sourceLocation).parent();
  while (!loc.isRoot) {
    const locStr = loc.toString();
    const op = findOperation(model.componentGrid, locStr);
    const containingArray = findParentArray(model.componentGrid, locStr);
    if (op == null || containingArray == null) break;
    chain.push({ op, containingArray });
    loc = loc.parent();
  }
  return chain;
};

/**
 * Collect the destination's ancestor chain, innermost-first, up to
 * (but not including) the root grid. Used by the dest-side
 * ancestor refresh cascade at the tail of `moveOperation`.
 *
 * `targetLocation` here addresses the *slot* the source is about
 * to be inserted into (e.g. `"0,0-1,0"` = "put me in scope 0,0 at
 * column 1, opIndex 0"). The innermost destination ancestor is the
 * op the dropzone's scope belongs to — i.e. the op at the prefix
 * before the last `-` (`"0,0"` in the example). We walk up from
 * there.
 *
 * Same object-reference contract as `_collectAncestorChain`: post-
 * move, `targetLocation` may name a different op (column splices,
 * empty-prune cascades), so we lock in references now.
 */
const _collectDestAncestorChain = (
  model: CircuitModel,
  targetLocation: string,
): AncestorRung[] => {
  const chain: AncestorRung[] = [];
  // Innermost destination ancestor = the scope op of the dropzone.
  // `targetLocation`'s parent IS that scope op's location.
  let loc = Location.parse(targetLocation).parent();
  while (!loc.isRoot) {
    const locStr = loc.toString();
    const op = findOperation(model.componentGrid, locStr);
    const containingArray = findParentArray(model.componentGrid, locStr);
    if (op == null || containingArray == null) break;
    chain.push({ op, containingArray });
    loc = loc.parent();
  }
  return chain;
};

/**
 * Walk the circuit tree and collect the ancestor chain (innermost-
 * first, up to but not including the root grid) that leads to the
 * specified `target` op — comparing by object identity, not by
 * location string. Used by mutators like
 * [`addControl`](#)/[`removeControl`](#) that receive an `Operation`
 * reference but no location.
 *
 * Returns `[]` if `target` is a top-level op (or not found at all
 * — callers should treat "not found" identically to "top-level":
 * either way, there are no ancestors to refresh).
 */
const _findAncestorChainForOp = (
  model: CircuitModel,
  target: Operation,
): AncestorRung[] => {
  const walk = (
    grid: ComponentGrid,
    chain: AncestorRung[],
  ): AncestorRung[] | null => {
    for (const col of grid) {
      for (const op of col.components) {
        if (op === target) return chain;
        if (op.children != null) {
          const found = walk(op.children, [
            { op, containingArray: grid },
            ...chain,
          ]);
          if (found != null) return found;
        }
      }
    }
    return null;
  };
  return walk(model.componentGrid, []) ?? [];
};

/**
 * Post-order deep refresh of every group's derived `.targets` /
 * `.results` in `grid`. Used by batch mutators like
 * [`findAndRemoveOperations`](#) that may have stripped ops from
 * many different ancestor chains in one pass — too many separate
 * chains to track individually, so we just re-derive every
 * group's cache in a single bottom-up sweep.
 *
 * Post-order is essential: a parent's cache is recomputed from
 * its immediate children's `.targets`, which must already reflect
 * the post-mutation state. Walking bottom-up ensures children are
 * refreshed before any parent reads them.
 *
 * Narrowing-only — no overlap-resolver pass. Batch removal can
 * only shrink ancestor spans, never widen them, so no new
 * sibling-column collisions can appear.
 */
const _deepRefreshDerivedTargets = (grid: ComponentGrid): void => {
  for (const col of grid) {
    for (const op of col.components) {
      if (op.children != null) {
        _deepRefreshDerivedTargets(op.children);
        _refreshDerivedTargets(op);
      }
    }
  }
};

/**
 * Sibling-collision resolver for the extend cascade.
 *
 * After `_refreshDerivedTargets` widens `op`'s `.targets`, its
 * register span may now overlap one or more siblings in the same
 * column. The renderer can't draw two ops on the same column whose
 * spans intersect, so we follow the existing
 * [`commitAddControl`](../editor/controllers/dragController.ts)
 * convention: splice `op` out of its current column and insert a
 * brand-new column containing only `op` at the SAME column index.
 * That pushes the old column (with the surviving siblings) one
 * slot to the right of `op`, restoring a non-overlapping layout
 * without disturbing any siblings' relative order.
 *
 * No-op when:
 *   - the column has no siblings (nothing to overlap),
 *   - no sibling actually overlaps `op`'s (possibly widened) span,
 *   - `op` can't be located in `containingArray` (e.g. it was
 *     already pruned upstream — defensive guard, shouldn't fire).
 *
 * Symmetric to `resolveOverlappingOperations` but operates on a
 * single known op rather than scanning the whole grid: the cascade
 * already knows which op was just widened and which array contains
 * it, so a targeted resolve is cheaper than a full grid sweep.
 */
const _resolveOverlapAfterExtend = (
  op: Operation,
  containingArray: ComponentGrid,
): void => {
  // Locate `op` in `containingArray`.
  let columnIndex = -1;
  let position = -1;
  for (let c = 0; c < containingArray.length; c++) {
    const idx = containingArray[c].components.indexOf(op);
    if (idx >= 0) {
      columnIndex = c;
      position = idx;
      break;
    }
  }
  if (columnIndex < 0) return;

  const column = containingArray[columnIndex];
  if (column.components.length <= 1) return;

  const [opMin, opMax] = _getMinMaxRegIdx(op);
  let collides = false;
  for (let i = 0; i < column.components.length; i++) {
    if (i === position) continue;
    const [sMin, sMax] = _getMinMaxRegIdx(column.components[i]);
    if (_doesOverlap([opMin, opMax], [sMin, sMax])) {
      collides = true;
      break;
    }
  }
  if (!collides) return;

  // Splice `op` out and insert a fresh column containing only `op`
  // at the same index — this pushes the surviving siblings one
  // column to the right of `op`.
  column.components.splice(position, 1);
  containingArray.splice(columnIndex, 0, { components: [op] });
};

/**
 * `true` if `op` has no rendered content underneath it — either no
 * `children` at all, or `children` that are all empty columns. An
 * empty group has no semantic meaning (nothing to execute) and no
 * sensible render (zero width, zero height); the cleanup pass in
 * `moveOperation` deletes such groups rather than leaving them as
 * landmines for the renderer.
 */
const _isOperationEmpty = (op: Operation): boolean => {
  if (op.children == null || op.children.length === 0) return true;
  return op.children.every((col) => col.components.length === 0);
};

/**
 * Dedup a `Register[]` by full `(qubit, result)` identity, returning
 * fresh `Register` objects in canonical `(qubit, result)` order:
 * qubit-only refs (`result === undefined`) sort before that qubit's
 * classical-result refs, then classical refs by ascending `result`.
 *
 * Bare-qubit `{qubit:N}` and classical-ref `{qubit:N, result:M}` are
 * distinct identities; both survive. Outputs are fresh objects so
 * callers can assign directly into `op.targets` / `op.results`.
 */
const _dedupRegistersByIdentity = (registers: Register[]): Register[] => {
  const seen = new Set<string>();
  const out: Register[] = [];
  for (const reg of registers) {
    const key =
      reg.result === undefined
        ? `${reg.qubit}:q`
        : `${reg.qubit}:c${reg.result}`;
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(
      reg.result === undefined
        ? { qubit: reg.qubit }
        : { qubit: reg.qubit, result: reg.result },
    );
  }
  out.sort((a, b) => {
    if (a.qubit !== b.qubit) return a.qubit - b.qubit;
    if (a.result === undefined && b.result === undefined) return 0;
    if (a.result === undefined) return -1;
    if (b.result === undefined) return 1;
    return a.result - b.result;
  });
  return out;
};

/**
 * Compute `op`'s derived `.targets` (or `.results`) from the
 * union of its **immediate children's** contributions.
 *
 * Each child contributes the union of its own register-bearing
 * fields (`getOperationRegisters`: targets + controls for
 * unitaries, qubits + results for measurements, targets for
 * kets). Because every group's `.targets` is itself the eager
 * cache of its own subtree, taking only the immediate children
 * here gives the same answer as recursively walking the
 * subtree — but without paying the recursion cost at every
 * ancestor level.
 *
 * Returns `[]` when `op` has no children grid; callers should
 * not invoke this on leaf ops.
 */
const _computeDerivedTargets = (op: Operation): Register[] => {
  if (op.children == null) return [];
  const registers: Register[] = [];
  for (const col of op.children) {
    for (const child of col.components) {
      registers.push(...getOperationRegisters(child));
    }
  }
  return _dedupRegistersByIdentity(registers);
};

/**
 * Order-sensitive equality on `Register[]`. Used by the
 * ancestor cascade to decide whether a refresh actually changed
 * the cached value — positional compare is safe because
 * `_computeDerivedTargets` produces stable child-iteration-order
 * output.
 */
const _registerListsEqual = (a: Register[], b: Register[]): boolean => {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) {
    if (a[i].qubit !== b[i].qubit) return false;
    if (a[i].result !== b[i].result) return false;
  }
  return true;
};

/**
 * Refresh the derived `.targets` / `.results` of `op` from its
 * immediate children. Returns `true` iff the cached value
 * actually changed — the ancestor cascade in
 * `refreshAncestorTargets` uses this signal to terminate.
 *
 * No-op (returns `false`) for ops with no `children` grid;
 * those are leaf ops whose `.targets` is explicit, not derived.
 */
const _refreshDerivedTargets = (op: Operation): boolean => {
  if (op.children == null) return false;
  const newValue = _computeDerivedTargets(op);
  if (op.kind === "measurement") {
    if (_registerListsEqual(op.results, newValue)) return false;
    op.results = newValue;
    return true;
  }
  if (op.kind === "unitary" || op.kind === "ket") {
    if (_registerListsEqual(op.targets, newValue)) return false;
    op.targets = newValue;
    return true;
  }
  return false;
};

/**
 * Walk an ancestor chain innermost-out and refresh each
 * still-attached ancestor's derived `.targets` / `.results`
 * from its immediate children. Stops at the first ancestor
 * whose cached value didn't actually change — at that point
 * nothing higher up can change either, because every higher
 * ancestor's cache depends only on its own immediate
 * children's caches.
 *
 * This is the **single centralized mechanism** for keeping the
 * eager `.targets` cache in sync with the children, used by
 * both the source-side post-prune refresh and the dest-side
 * post-insert refresh in `moveOperation`. See D7 in
 * [`CIRCUIT_EDITOR_TODO.md`](../CIRCUIT_EDITOR_TODO.md) for the
 * rationale and the per-call-site history this replaces.
 *
 * # Why immediate children only
 *
 * Each child's `.targets` is **already** the correct eager
 * cache of its own subtree — every prior refresh maintained
 * that invariant. So a parent's union is just the union of its
 * immediate children's contributions, no recursion needed. The
 * naive alternative (`getChildTargets` walks the whole subtree
 * top-down on every ancestor) costs O(subtree-size) per
 * ancestor, which compounds to O(depth × subtree) across the
 * chain — wasted work, because each level repeats what the
 * level below it already finished.
 *
 * # Contract
 *
 *   - `chain` is innermost-first: index 0 is the deepest
 *     ancestor, last index is closest to the root grid.
 *   - Rungs whose `op` is no longer attached to its captured
 *     `containingArray` are silently skipped — the empty-prune
 *     pass may have removed them between capture and refresh.
 *     The walk continues to the next-attached rung (whose
 *     children list changed because the detached rung was
 *     removed).
 *   - Refresh is **deterministic and idempotent**: calling this
 *     twice in a row on the same chain has no observable effect
 *     on the second call (every refresh on the second pass
 *     returns "unchanged" immediately).
 *   - The walk does **no DOM work, no column reshape, no
 *     model-level structural edits**. Callers that need to
 *     react to a widened span — e.g. split a column when an
 *     ancestor's new span now overlaps a sibling — pass
 *     `onAfterRefresh` to compose that on top. The hook fires
 *     on **every** visited still-attached rung, regardless of
 *     whether the refresh changed `.targets`: a shared
 *     ancestor between source and dest chains can have its
 *     value written by the first cascade to reach it, leaving
 *     the second to see "no change" — but the span still
 *     widened relative to pre-mutation state, so the hook
 *     (e.g. overlap resolver) must still run. The hook is
 *     expected to be idempotent under that no-op case.
 *
 * # Why the chain is captured before mutation
 *
 * Callers (`_collectAncestorChain` / `_collectDestAncestorChain`)
 * walk `Location` strings during capture, then hold the resulting
 * `(op, containingArray)` object references for the duration of
 * the mutation. Hierarchical location strings can be invalidated
 * by mid-move column splices or empty-prune cascades; object
 * references survive those shifts because they identify the
 * actual array and op without traversing the tree.
 *
 * # Termination is symmetric across widening and narrowing
 *
 * Works for both narrowing (source-side: a child was removed,
 * so spans may shrink) and widening (dest-side: a child was
 * inserted, so spans may grow) refreshes. The
 * cache-value-unchanged check fires whenever this ancestor's
 * recomputed value matches its prior cache — that's the
 * "nothing higher up can change" stop condition for both
 * directions, since every higher ancestor reads only its own
 * immediate children's `.targets`.
 */
const refreshAncestorTargets = (
  chain: AncestorRung[],
  options: { onAfterRefresh?: (rung: AncestorRung) => void } = {},
): void => {
  for (const rung of chain) {
    const { op, containingArray } = rung;

    // Skip rungs that were detached between capture and refresh
    // (e.g. the empty-prune pass spliced them out). Keep walking —
    // the next-attached ancestor's children list changed because
    // the detached rung was removed, so it still needs refreshing.
    let stillAttached = false;
    for (const col of containingArray) {
      if (col.components.indexOf(op) >= 0) {
        stillAttached = true;
        break;
      }
    }
    if (!stillAttached) continue;

    const changed = _refreshDerivedTargets(op);
    // Hook fires on every visited still-attached rung — see
    // the "shared ancestor" note in the doc comment for why
    // this can't be gated on `changed`.
    options.onAfterRefresh?.(rung);
    if (!changed) return;
  }
};

/**
 * Walk the ancestor chain innermost-out and delete any ancestor
 * whose children just collapsed to empty. Deletion is cascading:
 * if removing an ancestor empties its grandparent (because that
 * grandparent only contained this one ancestor), the grandparent
 * gets deleted next.
 *
 * Why this is necessary. `moveOperation` removes the source op
 * from its parent's grid, but the parent group itself is left in
 * place even if it now has no children. The renderer trips on
 * empty groups (zero-wire layout, undefined targets), and even if
 * it didn't, an empty group has no semantic meaning — there's
 * nothing to execute, no Q# to emit. Quietly deleting it matches
 * what users intuitively expect when they "move the last thing
 * out of" a group.
 *
 * Returns the surviving (still-attached) rungs of the chain in
 * innermost-first order. The caller hands this back to
 * `refreshAncestorTargets` so the first non-empty ancestor — and
 * any higher ancestors whose span still doesn't enclose what's
 * below it — get their derived `.targets` updated to reflect the
 * post-prune child set.
 *
 * Note on `qubitUseCounts` drift. We don't adjust use-counts when
 * deleting an empty group because the group itself contributed no
 * use-count entries (the count is per leaf-op, and the empty group
 * has no leaves). `removeTrailingUnusedQubits` is the safety net.
 */
const _pruneEmptyAncestors = (chain: AncestorRung[]): AncestorRung[] => {
  const survived: AncestorRung[] = [];
  let stillPruning = true;
  for (const rung of chain) {
    if (!stillPruning) {
      // Above the first non-empty ancestor — nothing here can
      // have been emptied by our move, but keep these rungs
      // around so the post-prune refresh walk can early-exit
      // through them cleanly.
      survived.push(rung);
      continue;
    }
    if (!_isOperationEmpty(rung.op)) {
      // First non-empty rung terminates the prune phase. It
      // survives (its children changed; refresh handles its
      // .targets) and so does everything above it.
      stillPruning = false;
      survived.push(rung);
      continue;
    }
    // Splice rung.op out of its containing array. Mirror the
    // column-cleanup convention from `_removeOp`: if the column
    // that held `op` is now empty, drop the column too.
    for (let colIdx = 0; colIdx < rung.containingArray.length; colIdx++) {
      const col = rung.containingArray[colIdx];
      const opIdx = col.components.indexOf(rung.op);
      if (opIdx >= 0) {
        col.components.splice(opIdx, 1);
        if (col.components.length === 0) {
          rung.containingArray.splice(colIdx, 1);
        }
        break;
      }
    }
    // Don't push the deleted rung to `survived`.
  }
  return survived;
};

/**
 * Move an operation vertically by changing its controls and targets.
 *
 * Pure mutator on `sourceOperation` — no grid walks, no model
 * touches. The parent-operation `targets`/`results` refresh that
 * used to live at the tail of this function is now done at the end
 * of `moveOperation` instead, so it runs against the post-removal
 * children grid (otherwise the parent would keep claiming the
 * departed child's wires).
 *
 * Two semantics, picked per-op by [`_moveAsUnit`](#):
 *
 * 1. **Unit-shift** for multi-wire ops (groups, SWAP, multi-qubit
 *    measurement). The grabbed wire acts as a handle: every
 *    register on the op (and recursively every register inside
 *    `children`, with external classical refs anchored — see
 *    [`_shiftAllRegisters`](#)) shifts by `targetWire - sourceWire`.
 *    The whole op slides as a rigid unit, preserving the relative
 *    arrangement of its wires.
 *
 * 2. **Single-leg rewire** for ordinary controlled-gate cases (one
 *    target + N controls). Only the grabbed register is rewritten;
 *    the other legs stay put. This is the established "rewire one
 *    leg of a CNOT" interaction.
 *
 * **Design decision (D3 in [CIRCUIT_EDITOR_TODO.md](../CIRCUIT_EDITOR_TODO.md)).**
 * The "grabbed wire is the handle" model was picked over two
 * alternatives we considered:
 *
 *   - **Pin lowest wire to drop wire** ("drop wire = top of
 *     group"). Predictable for the "I want this group at wires
 *     2..5" mental model, but discards the user's choice of which
 *     wire to grab. Bad match for the
 *     direct-manipulation feel — clicking on wire 4 of a group and
 *     dragging it to wire 6 should pin wire 4 to wire 6, not pin
 *     the topmost wire.
 *   - **Resize** (one leg moves, others stay). Only meaningful for
 *     ops with a clear "main" wire (CNOT target/controls); not
 *     applicable to groups. Conflicts with single-leg rewire when
 *     there's only one target. Better expressed via an explicit
 *     Inspector / right-click action than as a drag gesture.
 *
 * Richer multi-target authoring (resize, add/remove leg) belongs
 * in the Inspector, not the drag-and-drop surface.
 */
const _moveY = (
  sourceOperation: Operation,
  sourceWire: number,
  targetWire: number,
  movingControl: boolean,
): void => {
  // Group / multi-target / multi-qubit ops: move the whole gate as
  // a unit (shift every register by the same delta). See
  // `_moveAsUnit` for the criteria and rationale.
  if (_moveAsUnit(sourceOperation, movingControl)) {
    const delta = targetWire - sourceWire;
    if (delta !== 0) _shiftAllRegisters(sourceOperation, delta);
    return;
  }

  // Single-leg path (CNOT-style: rewire just one target or one
  // control leg). Everything below this point is the original
  // single-target movement logic.

  // Check if the source operation already has a target or control on the target wire
  let targets: Register[];
  switch (sourceOperation.kind) {
    case "unitary":
    case "ket":
      targets = sourceOperation.targets;
      break;
    case "measurement":
      targets = sourceOperation.qubits;
      break;
  }

  let controls: Register[];
  switch (sourceOperation.kind) {
    case "unitary":
      controls = sourceOperation.controls || [];
      break;
    case "measurement":
    case "ket":
      controls = [];
      break;
  }

  let likeRegisters: Register[];
  let unlikeRegisters: Register[];
  if (movingControl) {
    likeRegisters = controls;
    unlikeRegisters = targets;
  } else {
    likeRegisters = targets;
    unlikeRegisters = controls;
  }

  // If a similar register already exists, don't move the gate
  if (likeRegisters.find((reg) => reg.qubit === targetWire)) {
    return;
  }

  // If a different kind of register already exists, swap the control and target
  if (unlikeRegisters.find((reg) => reg.qubit === targetWire)) {
    const index = unlikeRegisters.findIndex((reg) => reg.qubit === targetWire);
    unlikeRegisters[index].qubit = sourceWire;
  }

  switch (sourceOperation.kind) {
    case "unitary":
      if (movingControl) {
        sourceOperation.controls?.forEach((control) => {
          if (control.qubit === sourceWire) {
            control.qubit = targetWire;
          }
        });
        sourceOperation.controls = sourceOperation.controls?.sort(
          (a, b) => a.qubit - b.qubit,
        );
      } else {
        sourceOperation.targets = [{ qubit: targetWire }];
      }
      break;
    case "measurement":
      sourceOperation.qubits = [{ qubit: targetWire }];
      // The measurement result is updated later in the _updateMeasurementLines function
      break;
    case "ket":
      sourceOperation.targets = [{ qubit: targetWire }];
      break;
  }
};

/**
 * Add an operation into the circuit.
 *
 * @returns The added operation or null if the addition was unsuccessful.
 */
const addOperation = (
  model: CircuitModel,
  sourceOperation: Operation,
  targetLocation: string,
  targetWire: number,
  insertNewColumn: boolean = false,
): Operation | null => {
  const targetOperationParent = findParentArray(
    model.componentGrid,
    targetLocation,
  );
  const targetLastIndex = Location.parse(targetLocation).last();

  if (targetOperationParent == null || targetLastIndex == null) return null;
  // Create a deep copy of the source operation
  const newSourceOperation: Operation = JSON.parse(
    JSON.stringify(sourceOperation),
  );

  if (newSourceOperation.kind === "measurement") {
    newSourceOperation.qubits = [{ qubit: targetWire }];
    // The measurement result is updated later in the _updateMeasurementLines function
  } else if (
    newSourceOperation.kind === "unitary" ||
    newSourceOperation.kind === "ket"
  ) {
    newSourceOperation.targets = [{ qubit: targetWire }];
  }

  model.ensureQubitCount(targetWire);

  // Capture the dest ancestor chain BEFORE _addOp so the rung
  // references survive any column splices `_addOp` may perform.
  // Empty when targetLocation is top-level (parent is root).
  const destAncestorChain: AncestorRung[] = _collectDestAncestorChain(
    model,
    targetLocation,
  );

  _addOp(
    model,
    newSourceOperation,
    targetOperationParent,
    targetLastIndex,
    insertNewColumn,
  );

  // After mutating the parent group's children, its derived
  // `.targets` (and every ancestor above it) must be re-derived
  // from the children. Cascade up, with the same overlap-resolver
  // hook `moveOperation` uses on its dest side: a widened ancestor
  // can collide with a sibling in its containing column, and the
  // resolver splits the column to keep layout legal.
  refreshAncestorTargets(destAncestorChain, {
    onAfterRefresh: ({ op, containingArray }) =>
      _resolveOverlapAfterExtend(op, containingArray),
  });

  return newSourceOperation;
};

/**
 * Remove an operation from the circuit.
 */
const removeOperation = (model: CircuitModel, sourceLocation: string) => {
  const sourceOperation = findOperation(model.componentGrid, sourceLocation);
  const sourceOperationParent = findParentArray(
    model.componentGrid,
    sourceLocation,
  );

  if (sourceOperation == null || sourceOperationParent == null) return null;

  // Capture the source ancestor chain BEFORE _removeOp so the rung
  // references survive the splice (and any column collapse that
  // follows when the source was the last op in its column).
  const ancestorChain = _collectAncestorChain(model, sourceLocation);

  _removeOp(model, sourceOperation, sourceOperationParent);

  // After mutating the parent group's children, its derived
  // `.targets` (and every ancestor above it) must be re-derived
  // from the surviving children. Narrowing-only cascade: no
  // overlap-resolver hook needed because shrinking an ancestor's
  // span can't introduce new sibling collisions.
  refreshAncestorTargets(ancestorChain);

  model.removeTrailingUnusedQubits();
};

/**
 * Find and remove operations in-place that return `true` for a predicate function.
 */
const findAndRemoveOperations = (
  model: CircuitModel,
  pred: (op: Operation) => boolean,
) => {
  // Remove operations that are true for the predicate function
  const inPlaceFilter = (grid: ComponentGrid) => {
    let i = 0;
    while (i < grid.length) {
      let j = 0;
      while (j < grid[i].components.length) {
        const op = grid[i].components[j];
        if (op.children) {
          inPlaceFilter(op.children);
        }
        if (pred(op)) {
          model.decrementQubitUseCountForOp(op);
          grid[i].components.splice(j, 1);
        } else {
          j++;
        }
      }
      if (grid[i].components.length === 0) {
        grid.splice(i, 1);
      } else {
        i++;
      }
    }
  };

  inPlaceFilter(model.componentGrid);

  // Batch removal may have stripped ops from many different ancestor
  // chains — too many to track individually — so re-derive every
  // group's cache in a single bottom-up sweep. Narrowing-only.
  _deepRefreshDerivedTargets(model.componentGrid);
};

/**
 * Add a control to the specified operation on the given wire index.
 *
 * @returns True if the control was added, false if it already existed.
 */
const addControl = (
  model: CircuitModel,
  op: Unitary,
  wireIndex: number,
): boolean => {
  if (!op.controls) {
    op.controls = [];
  }
  const existingControl = op.controls.find(
    (control) => control.qubit === wireIndex,
  );
  if (!existingControl) {
    // Capture ancestors before mutating so the rung references
    // survive any column splices the overlap-resolver may perform.
    const ancestorChain = _findAncestorChainForOp(model, op);

    op.controls.push({ qubit: wireIndex });
    op.controls.sort((a, b) => a.qubit - b.qubit);
    model.ensureQubitCount(wireIndex);
    model.qubitUseCounts[wireIndex]++;

    // Adding a control on a wire outside the op's existing span
    // widens it, which propagates into every ancestor group's
    // derived `.targets`. Overlap-resolver hook handles any new
    // sibling-column collisions the widening introduces.
    refreshAncestorTargets(ancestorChain, {
      onAfterRefresh: ({ op: ancestorOp, containingArray }) =>
        _resolveOverlapAfterExtend(ancestorOp, containingArray),
    });
    return true;
  }
  return false;
};

/**
 * Remove a control from the specified operation on the given wire index.
 *
 * @returns True if the control was removed, false if it did not exist.
 */
const removeControl = (
  model: CircuitModel,
  op: Unitary,
  wireIndex: number,
): boolean => {
  if (op.controls) {
    const controlIndex = op.controls.findIndex(
      (control) => control.qubit === wireIndex,
    );
    if (controlIndex !== -1) {
      // Capture ancestors before mutating; even though narrowing
      // can't trigger column splices, we follow the same pattern
      // as the other mutators for consistency.
      const ancestorChain = _findAncestorChainForOp(model, op);

      op.controls.splice(controlIndex, 1);
      model.qubitUseCounts[wireIndex]--;
      if (wireIndex === model.qubits.length - 1) {
        model.removeTrailingUnusedQubits();
      }

      // Narrowing only — no overlap-resolver hook needed.
      refreshAncestorTargets(ancestorChain);
      return true;
    }
  }
  return false;
};

/**
 * Move a qubit line from `sourceWire` to `targetWire`. Two modes:
 *
 *   - `isBetween: true`  — insert before `targetWire` (drop "between" wires).
 *   - `isBetween: false` — swap with `targetWire`.
 *
 * Updates qubit IDs, every operation's register references, sorts each
 * column by lowest-numbered register, and re-resolves any overlaps that
 * the rewire produced.
 *
 * No-op if `sourceWire === targetWire` or either is null/undefined.
 */
const moveQubit = (
  model: CircuitModel,
  sourceWire: number,
  targetWire: number,
  isBetween: boolean,
): void => {
  if (sourceWire === targetWire || sourceWire == null || targetWire == null) {
    return;
  }

  if (isBetween) {
    // Moving sourceWire to just before targetWire.
    let insertAt = targetWire;
    // If moving down and passing over itself, adjust index.
    if (sourceWire < insertAt) insertAt--;
    _moveArrayElement(model.qubits, sourceWire, insertAt);
    _moveArrayElement(model.qubitUseCounts, sourceWire, insertAt);
  } else {
    // Swap sourceWire and targetWire.
    [model.qubits[sourceWire], model.qubits[targetWire]] = [
      model.qubits[targetWire],
      model.qubits[sourceWire],
    ];
    [model.qubitUseCounts[sourceWire], model.qubitUseCounts[targetWire]] = [
      model.qubitUseCounts[targetWire],
      model.qubitUseCounts[sourceWire],
    ];
  }

  // Update qubit ids to match their new positions
  model.qubits.forEach((q, idx) => {
    q.id = idx;
  });

  // Update all operations in componentGrid to reflect new qubit order
  for (const column of model.componentGrid) {
    for (const op of column.components) {
      getOperationRegisters(op).forEach((reg) => {
        if (isBetween) {
          // Move: update qubit indices
          if (reg.qubit === sourceWire) {
            reg.qubit = sourceWire < targetWire ? targetWire - 1 : targetWire;
          } else if (
            sourceWire < targetWire &&
            reg.qubit > sourceWire &&
            reg.qubit < targetWire
          ) {
            reg.qubit -= 1;
          } else if (
            sourceWire > targetWire &&
            reg.qubit >= targetWire &&
            reg.qubit < sourceWire
          ) {
            reg.qubit += 1;
          }
        } else {
          // Swap: swap indices
          if (reg.qubit === sourceWire) reg.qubit = targetWire;
          else if (reg.qubit === targetWire) reg.qubit = sourceWire;
        }
      });
    }
    // Sort operations in this column by their lowest-numbered register
    column.components.sort((a, b) => {
      const aRegs = getOperationRegisters(a);
      const bRegs = getOperationRegisters(b);
      const aMin = Math.min(...aRegs.map((r) => r.qubit));
      const bMin = Math.min(...bRegs.map((r) => r.qubit));
      return aMin - bMin;
    });
  }

  resolveOverlappingOperations(model.componentGrid);
  model.removeTrailingUnusedQubits();
};

/**
 * Remove a qubit line at `qubitIdx`. Caller is responsible for asking
 * the user to confirm if the wire still has operations on it; this
 * function only does the data mutation.
 *
 * Decrements all references on higher-numbered wires by 1 (since their
 * indices shift down) and renumbers qubit ids to match. Operations
 * that touched `qubitIdx` are **not** removed by this call — caller
 * should `findAndRemoveOperations` first if that's the intent.
 */
const removeQubit = (model: CircuitModel, qubitIdx: number): void => {
  model.qubits.splice(qubitIdx, 1);
  model.qubitUseCounts.splice(qubitIdx, 1);
  model.removeTrailingUnusedQubits();

  // Update all operation references throughout the tree — including
  // ops nested inside groups, AND the eager `.targets` / `.results`
  // caches on those groups (which hold their own Register objects
  // independent of descendant ops). Walking recursively keeps both
  // child refs and cached refs in lockstep, so the uniform shift
  // preserves cache coherence without needing a separate refresh.
  const shiftRefsInGrid = (grid: ComponentGrid): void => {
    for (const column of grid) {
      for (const op of column.components) {
        getOperationRegisters(op).forEach((reg) => {
          if (reg.qubit > qubitIdx) reg.qubit -= 1;
        });
        if (op.children != null) shiftRefsInGrid(op.children);
      }
    }
  };
  shiftRefsInGrid(model.componentGrid);

  // Update qubit ids to match their new positions
  model.qubits.forEach((q, idx) => {
    q.id = idx;
  });
};

/**
 * Resolves overlapping operations in each column of the component grid.
 * For each column, splits overlapping operations into separate columns so that
 * no two operations in the same column overlap on their register ranges.
 * Modifies the component grid in-place.
 */
const resolveOverlappingOperations = (parentArray: ComponentGrid): void => {
  // Helper to resolve a single column into non-overlapping columns
  const resolveColumn = (col: Column): Column[] => {
    const newColumn: Column = { components: [] };
    let [lastMin, lastMax] = [-1, -1];
    let i = 0;
    while (i < col.components.length) {
      const op = col.components[i];
      const [currMin, currMax] = _getMinMaxRegIdx(op);
      // Sets up the first operation for comparison or if the current operation doesn't overlap
      if (i === 0 || !_doesOverlap([lastMin, lastMax], [currMin, currMax])) {
        [lastMin, lastMax] = [currMin, currMax];
        i++;
      } else {
        // If they overlap, add the current operation to the new column
        newColumn.components.push(op);
        col.components.splice(i, 1);
      }
    }
    if (newColumn.components.length > 0) {
      const newColumns = resolveColumn(newColumn);
      newColumns.push(col);
      return newColumns;
    } else {
      return [col];
    }
  };

  // In-place update of parentArray
  let i = 0;
  while (i < parentArray.length) {
    const col = parentArray[i];
    const newColumns = resolveColumn(col);
    if (newColumns.length > 1) {
      parentArray.splice(i, 1, ...newColumns);
      i += newColumns.length;
    }
    i++;
  }
};

/** Determines whether two register index ranges overlap. */
const _doesOverlap = (
  op1: [number, number],
  op2: [number, number],
): boolean => {
  const [min1, max1] = op1;
  const [min2, max2] = op2;
  return max1 >= min2 && max2 >= min1;
};

/** Move an element of `arr` from index `from` to index `to`. */
const _moveArrayElement = <T>(arr: T[], from: number, to: number) => {
  const el = arr.splice(from, 1)[0];
  arr.splice(to, 0, el);
};

/**
 * Add an operation to the circuit at the specified location.
 */
const _addOp = (
  model: CircuitModel,
  sourceOperation: Operation,
  targetOperationParent: ComponentGrid,
  targetLastIndex: readonly [number, number],
  insertNewColumn: boolean = false,
  originalOperation: Operation | null = null,
) => {
  const [colIndex, opIndex] = targetLastIndex;
  if (targetOperationParent[colIndex] == null) {
    targetOperationParent[colIndex] = { components: [] };
  }

  insertNewColumn =
    insertNewColumn || _isClassicallyControlled(sourceOperation);

  // Check if there are any existing operations in the target
  // column within the wire range of the new operation
  if (!insertNewColumn) {
    const [minTarget, maxTarget] = _getMinMaxRegIdx(sourceOperation);
    for (const op of targetOperationParent[colIndex].components) {
      if (op === originalOperation) continue;

      const [opMinTarget, opMaxTarget] = _getMinMaxRegIdx(op);
      if (_doesOverlap([minTarget, maxTarget], [opMinTarget, opMaxTarget])) {
        insertNewColumn = true;
        break;
      }
    }
  }

  if (insertNewColumn) {
    targetOperationParent.splice(colIndex, 0, {
      components: [sourceOperation],
    });
  } else {
    targetOperationParent[colIndex].components.splice(
      opIndex,
      0,
      sourceOperation,
    );
  }

  model.incrementQubitUseCountForOp(sourceOperation);

  if (sourceOperation.kind === "measurement") {
    for (const targetWire of sourceOperation.qubits) {
      _updateMeasurementLines(model, targetWire.qubit);
    }
  }
};

/**
 * Get the minimum and maximum register indices for a given operation.
 * Based on getMinMaxRegIdx in process.ts, but without the numQubits.
 */
const _getMinMaxRegIdx = (operation: Operation): [number, number] =>
  getQuantumWireRange(operation);

/** Check if an operation is classically controlled. */
const _isClassicallyControlled = (operation: Operation): boolean => {
  if (operation.kind !== "unitary") return false;
  if (operation.controls === undefined) return false;
  const clsControl = operation.controls.find(
    ({ result }) => result !== undefined,
  );
  return clsControl !== undefined;
};

/** Remove an operation from the circuit. */
const _removeOp = (
  model: CircuitModel,
  sourceOperation: Operation,
  sourceOperationParent: ComponentGrid,
) => {
  if (sourceOperation.dataAttributes === undefined) {
    sourceOperation.dataAttributes = { removed: "true" };
  } else {
    sourceOperation.dataAttributes["removed"] = "true";
  }

  // Find and remove the operation in sourceOperationParent
  for (let colIndex = 0; colIndex < sourceOperationParent.length; colIndex++) {
    const col = sourceOperationParent[colIndex];
    const indexToRemove = col.components.findIndex(
      (operation) =>
        operation.dataAttributes && operation.dataAttributes["removed"],
    );
    if (indexToRemove !== -1) {
      col.components.splice(indexToRemove, 1);
      if (col.components.length === 0) {
        sourceOperationParent.splice(colIndex, 1);
      }
      break;
    }
  }

  model.decrementQubitUseCountForOp(sourceOperation);

  if (sourceOperation.kind === "measurement") {
    for (const result of sourceOperation.results) {
      _updateMeasurementLines(model, result.qubit);
    }
  }
};

/**
 * Update measurement-result indices for a specific wire.
 *
 * Walks the **entire** grid tree (including nested children of
 * group ops) and renumbers every measurement on `wireIndex` in
 * document order. `model.qubits[wireIndex].numResults` is then
 * set to the total count.
 *
 * Recursing into children is essential because the renderer's
 * per-wire classical-register count comes from this counter and
 * the renderer reads ANY measurement's results — including ones
 * inside expanded groups. If a nested measurement's wire isn't
 * counted here, the renderer throws "Classical register ID N
 * invalid for qubit ID M with 0 classical register(s)" the next
 * time it tries to address the missing register.
 */
const _updateMeasurementLines = (model: CircuitModel, wireIndex: number) => {
  model.ensureQubitCount(wireIndex);
  let resultIndex = 0;
  const walk = (grid: ComponentGrid): void => {
    for (const col of grid) {
      for (const comp of col.components) {
        if (comp.kind === "measurement") {
          const qubit = comp.qubits.find((q) => q.qubit === wireIndex);
          if (qubit) {
            comp.results = [{ qubit: qubit.qubit, result: resultIndex++ }];
          }
        }
        if (comp.children) walk(comp.children);
      }
    }
  };
  walk(model.componentGrid);
  model.qubits[wireIndex].numResults =
    resultIndex > 0 ? resultIndex : undefined;
};

export {
  addControl,
  addOperation,
  collectExternalProducerLocations,
  findAndRemoveOperations,
  moveOperation,
  moveQubit,
  removeControl,
  removeOperation,
  removeQubit,
  resolveOverlappingOperations,
};
