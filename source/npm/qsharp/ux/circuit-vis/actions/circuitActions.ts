// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { Column, ComponentGrid, Operation, Unitary } from "../data/circuit.js";
import { CircuitModel } from "../data/circuitModel.js";
import { Location } from "../data/location.js";
import { Register } from "../data/register.js";
import {
  findOperation,
  findParentArray,
  getMinMaxRegIdx,
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

  // Capture the destination's containing array (the grid the
  // moved op will live in directly) pre-move. Falls back to the
  // top-level grid for top-level drops. Same rationale as the
  // chain capture: the dest array reference stays valid through
  // every column splice `_addOp` / dest-cascade may perform,
  // unlike the location string.
  const destContainingArray: ComponentGrid =
    findParentArray(model.componentGrid, targetLocation) ?? model.componentGrid;

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

  // Stamp the new op with a one-shot "previous location" marker so
  // [`Sqore.rebaseViewState`](../sqore.ts) can transfer the user's
  // expand/collapse state across this move. Object identity is lost
  // through the JSON deep-clone above — `newSourceOperation` is a
  // distinct reference from the one in `lastLocationMap` — so the
  // identity lookup in `rebaseViewState` will miss and naively map
  // the old location to `null`, dropping the ViewState entry. The
  // visible symptom is most striking on classically-controlled
  // groups: when no ViewState entry exists, the renderer's default
  // `hasClassicalControls && hasChildren` kicks back in, re-expanding
  // groups the user had explicitly collapsed. The stamp lets the
  // rebase find this op by its pre-move location as a fallback;
  // it's consumed (deleted) on the very next rebase so it never
  // leaks into the rendered SVG as a `data-*` attribute or
  // accumulates across edits. See [B11](../CIRCUIT_EDITOR_TODO.md).
  if (newSourceOperation.dataAttributes == null) {
    newSourceOperation.dataAttributes = {};
  }
  newSourceOperation.dataAttributes["sqore-prev-location"] = sourceLocation;

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

  // Dest-side cleanup. Centralized post-widening cascade:
  //   - The newly-moved op vs its own column siblings (redundant
  //     with `_addOp`'s pre-insert overlap check, but kept for
  //     architectural consistency — no-op when `_addOp` already
  //     resolved it).
  //   - Every dest ancestor whose `.targets` no longer encloses
  //     its child's (possibly widened) wire span gets re-derived,
  //     with the sibling-collision resolver firing on each.
  //
  // Always-on because the target location string is authoritative:
  // if the user dropped the source at a location inside group G,
  // then G IS the source's new parent, and G's `.targets` MUST
  // reflect that. No-op when the drop is top-level (empty chain)
  // or when every dest ancestor was pruned by the source-side
  // sweep above — see `refreshAncestorTargets`'s `stillAttached`
  // check.
  _resolveSpanChange(
    {
      op: newSourceOperation,
      containingArray: destContainingArray,
    },
    destAncestorChain,
  );

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
 *
 * `movingControl` takes precedence over the group check: a control
 * on a group is still a single leg (rewire just the control),
 * matching the established "drag a control to move only that
 * control" interaction. Without this short-circuit, dragging a
 * control on a group would shift the entire group as a unit and
 * the user's drop wire becomes the new group home — destroying
 * the intent of the gesture.
 */
const _moveAsUnit = (op: Operation, movingControl: boolean): boolean => {
  if (movingControl) return false;
  if (op.children != null) return true;
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
 * Swap every register reference on `wireA` with every reference on
 * `wireB` throughout `op`'s subtree (top-level + recursively into
 * children). Used by the group + `movingControl` branch in
 * `_moveY` to implement the "drop the control onto a body wire to
 * swap them" gesture: callers pass `op.children` directly so the
 * top-level group's own controls/targets are left for the caller
 * to update explicitly afterward.
 *
 * Classical-register entries (`{qubit, result}`) get the same
 * `qubit` swap as quantum ones — for the body-swap gesture, a
 * classical reference whose producer wire is itself being swapped
 * needs to move with it. (The external-producer "anchor in place"
 * rule from `_doShift` doesn't apply here because we're swapping
 * specific wires, not delta-shifting.)
 */
const _swapWiresInSubtree = (
  op: Operation,
  wireA: number,
  wireB: number,
): void => {
  for (const reg of getOperationRegisters(op)) {
    if (reg.qubit === wireA) reg.qubit = wireB;
    else if (reg.qubit === wireB) reg.qubit = wireA;
  }
  if (op.children) {
    for (const col of op.children) {
      for (const child of col.components) {
        _swapWiresInSubtree(child, wireA, wireB);
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
 * [`Location.parse`](../data/location.ts) consumes.
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
 *   [`getChildTargets`](../utils.ts) rebuilds by walking the
 *   subtree and dedup-merging every descendant's registers. If
 *   the group contains a classically-controlled child, the
 *   child's classical-ref **propagates up** into every ancestor
 *   group's `.targets` cache so the renderer can draw the
 *   group's visual span down to the classical wire.
 *
 * Treating those propagated `.targets` entries as consumption
 * would falsely flag every enclosing group as a consumer. The
 * cascade-delete in
 * [`removeMeasurementWithDependents`](#) would then wipe out
 * the entire ancestor group — including unrelated sibling
 * children that don't depend on the M at all. The user-visible
 * symptom is "deleting one M emptied my circuit." See B2 in
 * [CIRCUIT_EDITOR_TODO.md](../CIRCUIT_EDITOR_TODO.md).
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
 * Move a measurement that has downstream classical consumers,
 * propagating the effects to those consumers.
 *
 * Wraps [`moveOperation`](#) with the additional bookkeeping
 * required to keep the classical producer→consumer graph
 * consistent across the move. The caller (the editor's prompt
 * layer) is expected to have:
 *
 *   1. Called [`collectMeasurementConsumers`](#) on the M.
 *   2. Partitioned the result by
 *      [`Location.inEarlierColumnThan`](../data/location.ts)
 *      against `targetLocation` into:
 *      - **Survivors** — consumers whose column would still come
 *        strictly after the M's new column. Their classical refs
 *        get their `qubit` field remapped to track the M's new
 *        wire (and the M's new result index after the
 *        wire-level renumbering pass).
 *      - **Invalidated** — consumers whose column would end up
 *        at-or-before the M's new column. Document order would
 *        become inconsistent; these are passed in as
 *        `invalidatedConsumers` and cascade-deleted as part of
 *        the move.
 *   3. Confirmed the cascade with the user via a prompt.
 *
 * The handoff is intentional: the action layer stays UI-free, and
 * the partition / messaging logic stays in the controller. Tests
 * can exercise this function directly with a synthetic
 * `invalidatedConsumers` set.
 *
 * # Wire-remap detail
 *
 * `moveOperation`'s tail-end `_updateMeasurementLines` sweep
 * renumbers result indices on every affected wire, so we
 * snapshot every measurement's pre-move `(qubit, result)` keys
 * BEFORE the move, then compare with post-move keys to build a
 * complete remap. Consumers of OTHER Ms on the same wires that
 * got renumbered are picked up by the same remap — without that
 * sweep, B2's symptom would reappear in a different shape (a
 * consumer of an UNMOVED M pointing at a stale result index).
 *
 * The moved M becomes a NEW object reference (`moveOperation`
 * deep-clones the source), so we capture its pre-move keys
 * separately and pair them positionally with its post-move
 * keys on the returned op.
 *
 * # Ordering rationale
 *
 * Move first, then cascade-delete: lets us use the pre-move
 * `targetLocation` string directly (it's still valid against
 * the unmodified grid). Cascade-delete uses object-reference
 * predicates, which survive the move's column splices.
 *
 * Returns the moved M op (the new deep-clone reference), or
 * `null` if the underlying `moveOperation` refused the move
 * (the only documented refusal path is the negative-wire guard,
 * which can't fire for a single-qubit M move within bounds).
 */
const moveMeasurementWithDependents = (
  model: CircuitModel,
  sourceLocation: string,
  targetLocation: string,
  sourceWire: number,
  targetWire: number,
  insertNewColumn: boolean,
  invalidatedConsumers: Operation[],
): Operation | null => {
  const mOp = findOperation(model.componentGrid, sourceLocation);
  if (mOp == null || mOp.kind !== "measurement") return null;

  // Snapshot every M's pre-move (qubit, result) keys, indexed by
  // object identity. Other Ms whose result indices get renumbered
  // by the move's tail-end `_updateMeasurementLines` sweep need
  // their consumers updated too — same remap mechanism.
  const preMoveKeysByRef = new Map<
    Operation,
    { qubit: number; result: number }[]
  >();
  const walkMeasurements = (g: ComponentGrid): void => {
    for (const col of g) {
      for (const op of col.components) {
        if (op.kind === "measurement") {
          const list: { qubit: number; result: number }[] = [];
          for (const r of op.results) {
            if (r.result !== undefined) {
              list.push({ qubit: r.qubit, result: r.result });
            }
          }
          preMoveKeysByRef.set(op, list);
        }
        if (op.children) walkMeasurements(op.children);
      }
    }
  };
  walkMeasurements(model.componentGrid);

  // Pre-capture the moving M's pre-keys SEPARATELY because the
  // moved M is a new object post-move (deep-cloned by
  // `moveOperation`), so the by-ref map will have no entry for
  // it after the move.
  const movedMPreKeys = preMoveKeysByRef.get(mOp) ?? [];

  // Move M. The standard path handles wire change on M's own
  // registers + column placement + the global
  // `_updateMeasurementLines` renumbering at the tail.
  const movedM = moveOperation(
    model,
    sourceLocation,
    targetLocation,
    sourceWire,
    targetWire,
    /* movingControl */ false,
    insertNewColumn,
  );
  if (movedM == null) return null;

  // Cascade-delete invalidated consumers AFTER the move. Object
  // refs from `invalidatedConsumers` are still valid; their
  // locations may have shifted in the splice, but the predicate
  // matches on identity.
  const invalidatedSet = new Set(invalidatedConsumers);
  if (invalidatedSet.size > 0) {
    findAndRemoveOperations(model, (op) => invalidatedSet.has(op));
  }

  // Build the (oldQubit, oldResult) → (newQubit, newResult) remap
  // by pairing pre-move and post-move snapshots per M.
  //
  // - The moved M: pre-keys come from `movedMPreKeys` (snapshot
  //   pre-move); post-keys come from `movedM.results` (live on
  //   the returned new object).
  // - Every other M: pre-keys come from `preMoveKeysByRef`;
  //   post-keys come from the still-live op (object identity
  //   preserved).
  //
  // Pairing within an M is positional (entry-by-entry through
  // `results`). Single-qubit Ms have one entry each, which is
  // the common case; multi-qubit Ms keep entry order.
  const keyRemap = new Map<string, string>();
  const recordRemap = (
    preList: { qubit: number; result: number }[],
    postOp: Operation,
  ): void => {
    if (postOp.kind !== "measurement") return;
    const postList: { qubit: number; result: number }[] = [];
    for (const r of postOp.results) {
      if (r.result !== undefined) {
        postList.push({ qubit: r.qubit, result: r.result });
      }
    }
    const n = Math.min(preList.length, postList.length);
    for (let i = 0; i < n; i++) {
      const preKey = `${preList[i].qubit}:${preList[i].result}`;
      const postKey = `${postList[i].qubit}:${postList[i].result}`;
      if (preKey !== postKey) keyRemap.set(preKey, postKey);
    }
  };
  recordRemap(movedMPreKeys, movedM);
  for (const [preOp, preList] of preMoveKeysByRef) {
    if (preOp === mOp) continue; // moved M handled above
    recordRemap(preList, preOp);
  }

  // Apply the remap to every classical ref in the grid.
  // Walking the whole grid (not just the consumer set we
  // collected pre-move) catches consumers of OTHER Ms whose
  // result indices got bumped by the renumber sweep.
  if (keyRemap.size > 0) {
    _applyClassicalRefRemap(model.componentGrid, keyRemap);
  }

  // Consumers' visual spans may have changed (a classical-ref
  // wire moved), which can widen or narrow group `.targets`
  // caches and introduce new sibling-column collisions.
  // Re-derive bottom-up and resolve recursively, same pattern
  // `moveQubit` uses for the wire-remap case.
  _deepRefreshDerivedTargets(model.componentGrid);
  resolveOverlappingOperationsRecursive(model.componentGrid);

  return movedM;
};

/**
 * Remove a measurement and cascade-delete every op that depends
 * on its classical outputs.
 *
 * Same handoff contract as
 * [`moveMeasurementWithDependents`](#): the prompt layer
 * collects consumers, surfaces the count to the user, and on
 * confirm calls this with the consumer set.
 *
 * Two-step removal — cascade-delete consumers FIRST so that
 * `removeOperation`'s ancestor-targets refresh runs against a
 * grid that no longer carries dangling classical refs to the
 * M being deleted. The M's location may shift in the cascade
 * (consumers in earlier columns of the M's parent array can
 * collapse the column), so we look the M back up by object
 * reference before the final removal.
 *
 * # Result-index renumbering propagation
 *
 * `removeOperation`'s tail-end `_updateMeasurementLines` sweep
 * renumbers per-wire result indices to close the gap left by
 * the deleted M. If there were OTHER Ms on the same wire, their
 * result indices get bumped down — and consumers of those
 * unmoved Ms keep their stale `(qubit, result)` keys. The
 * renderer's layout pass then throws
 * `"Classical register ID N invalid for qubit ID M with X
 * classical register(s)"` because the consumer points at an
 * index that no longer exists.
 *
 * Fix: snapshot every surviving M's pre-removal keys by object
 * identity, do the removal, then build a remap from the
 * pre/post comparison (same mechanism as
 * [`moveMeasurementWithDependents`](#)) and apply it. The M
 * being deleted is excluded from the snapshot — its keys
 * shouldn't end up in the remap because nothing should resolve
 * to a deleted op.
 */
const removeMeasurementWithDependents = (
  model: CircuitModel,
  mLocation: string,
  consumers: Operation[],
): void => {
  const mOp = findOperation(model.componentGrid, mLocation);
  if (mOp == null) return;

  // Snapshot every OTHER M's pre-removal (qubit, result) keys,
  // indexed by object identity. The M being deleted is excluded
  // — its keys are about to vanish along with the op. Surviving
  // Ms on the same wire(s) will be renumbered by the tail-end
  // `_updateMeasurementLines` sweep inside `removeOperation`;
  // their consumers need a matching remap or the renderer
  // throws on the stale index.
  const preRemovalKeysByRef = new Map<
    Operation,
    { qubit: number; result: number }[]
  >();
  const walkMeasurements = (g: ComponentGrid): void => {
    for (const col of g) {
      for (const op of col.components) {
        if (op.kind === "measurement" && op !== mOp) {
          const list: { qubit: number; result: number }[] = [];
          for (const r of op.results) {
            if (r.result !== undefined) {
              list.push({ qubit: r.qubit, result: r.result });
            }
          }
          preRemovalKeysByRef.set(op, list);
        }
        if (op.children) walkMeasurements(op.children);
      }
    }
  };
  walkMeasurements(model.componentGrid);

  // Cascade-delete the consumers. Predicate matches on object
  // identity so we don't have to track location-string drift
  // across the splice.
  if (consumers.length > 0) {
    const consumerSet = new Set(consumers);
    findAndRemoveOperations(model, (op) => consumerSet.has(op));
  }

  // M's location may have shifted (its column may have lost
  // earlier-opIdx siblings to the cascade, or the column may
  // have collapsed and shifted). Re-derive by ref.
  const newMLoc = _findLocationByRef(model.componentGrid, mOp);
  if (newMLoc != null) {
    removeOperation(model, newMLoc);
  }

  // Build the remap by pairing pre/post snapshots positionally
  // per surviving M. An M that was itself cascade-deleted (e.g.
  // nested inside a deleted consumer group) is silently dropped:
  // it's gone from the grid, so we don't visit it during the
  // post-removal walk, and any keys it produced have no valid
  // target — there's nothing to remap to. That's correct: any
  // op still referencing such a result is either (a) also
  // cascade-deleted via the consumer set, or (b) about to throw
  // the same "invalid classical register" error, which is the
  // user's signal that the prompt didn't surface a dependency
  // they care about.
  const keyRemap = new Map<string, string>();
  for (const [postOp, preList] of preRemovalKeysByRef) {
    if (postOp.kind !== "measurement") continue;
    const postList: { qubit: number; result: number }[] = [];
    for (const r of postOp.results) {
      if (r.result !== undefined) {
        postList.push({ qubit: r.qubit, result: r.result });
      }
    }
    const n = Math.min(preList.length, postList.length);
    for (let i = 0; i < n; i++) {
      const preKey = `${preList[i].qubit}:${preList[i].result}`;
      const postKey = `${postList[i].qubit}:${postList[i].result}`;
      if (preKey !== postKey) keyRemap.set(preKey, postKey);
    }
  }

  if (keyRemap.size > 0) {
    _applyClassicalRefRemap(model.componentGrid, keyRemap);
    // Visual spans on surviving classically-controlled groups
    // may have shifted; refresh + resolve overlaps (same pattern
    // as the move path).
    _deepRefreshDerivedTargets(model.componentGrid);
    resolveOverlappingOperationsRecursive(model.componentGrid);
  }
};

/**
 * Walk the grid and remap every classical-ref entry's
 * `(qubit, result)` pair according to `keyRemap`. Visits both
 * the op's own register-bearing fields AND the cached
 * `.targets` / `.controls` on group ops (which hold their own
 * `Register` objects, independent of descendant ops — see
 * [`_dedupRegistersByIdentity`](#)).
 *
 * Only classical refs (`result !== undefined`) are touched;
 * quantum refs are left alone.
 */
const _applyClassicalRefRemap = (
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
        // the `_updateMeasurementLines` sweep that ran inside
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
const _findLocationByRef = (
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
 * Centralized post-widening cleanup for any single op whose
 * register span just changed (added control, added target, body
 * widened by a wire remap, etc.). Use this whenever a mutation
 * widens a specific op's `.targets` / `.controls` — it's the
 * single chokepoint so the "after I changed this op, make sure
 * everyone moves out of the way" invariant can't be missed at a
 * call site.
 *
 * Two-stage cleanup:
 *
 *   1. **Op itself.** Check the widened op against its own
 *      column siblings; if it now collides, splice it into a
 *      fresh column at the same index (same convention as the
 *      ancestor cascade below). This is the case the ancestor-
 *      only cascade misses: a top-level `addControl` that widens
 *      the op into a same-column sibling has NO ancestors, so
 *      the existing `onAfterRefresh` hook never fires. Same
 *      symptom at any nesting level — the immediate-children
 *      array's collisions are nobody's problem under the
 *      ancestor-only model.
 *   2. **Ancestor chain.** Re-derive each ancestor's `.targets`
 *      cache; if that widened the ancestor's span, resolve its
 *      own column collision via the same hook. Identical to
 *      what the existing `refreshAncestorTargets` cascade does;
 *      this routine just wraps the standard hook so every
 *      widening path uses it uniformly.
 *
 * Idempotent on the no-collision path: stage 1 is a one-pass
 * scan + no-op return; stage 2 early-exits on the first ancestor
 * whose cache didn't change. Safe to call from paths whose
 * earlier work (e.g. `_addOp`'s pre-insert overlap check) may
 * have already resolved the collision — the redundant call is
 * cheap and the architectural guarantee is worth more than the
 * micro-cost.
 *
 * @param opRung  The op being widened paired with its containing
 *   array (the grid one level above the op). Capture BEFORE the
 *   mutation if column splices may follow.
 * @param ancestorChain Innermost-first ancestor rungs above
 *   `opRung.op`. Empty when `opRung.op` is top-level.
 */
const _resolveSpanChange = (
  opRung: AncestorRung,
  ancestorChain: AncestorRung[],
): void => {
  _resolveOverlapAfterExtend(opRung.op, opRung.containingArray);
  refreshAncestorTargets(ancestorChain, {
    onAfterRefresh: ({ op, containingArray }) =>
      _resolveOverlapAfterExtend(op, containingArray),
  });
};

/**
 * Like [`_findAncestorChainForOp`](#) but ALSO captures the op's
 * own rung. Returned `opRung.containingArray` is the grid one
 * level above the op — i.e. the model's top-level grid for a
 * top-level op, or the parent group's `children` grid otherwise.
 *
 * Used by widening mutators (`addControl` and similar) that
 * receive an `Operation` reference and need to feed
 * [`_resolveSpanChange`](#).
 *
 * Returns `null` if the op isn't in the model. Callers should
 * treat that as a no-op (the mutation predicate can't fire on a
 * detached op).
 */
const _findOpRungAndAncestors = (
  model: CircuitModel,
  target: Operation,
): { opRung: AncestorRung; ancestorChain: AncestorRung[] } | null => {
  const walk = (
    grid: ComponentGrid,
    chain: AncestorRung[],
  ): { opRung: AncestorRung; ancestorChain: AncestorRung[] } | null => {
    for (const col of grid) {
      for (const op of col.components) {
        if (op === target) {
          return {
            opRung: { op, containingArray: grid },
            ancestorChain: chain,
          };
        }
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
  return walk(model.componentGrid, []);
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
    // Early-exit the refresh walk once nothing higher up can
    // change. EXCEPTION: when `onAfterRefresh` is provided, walk
    // the FULL chain so the hook fires on every ancestor. The
    // canonical case is `moveOperation`: its source-side cascade
    // (no hook) propagates the new spans up through every shared
    // ancestor, so the subsequent dest-side cascade (with hook)
    // sees `!changed` at the very first rung and would otherwise
    // skip the overlap-resolver on the higher ancestors where
    // the actual collision lives. The doc-comment contract
    // ("hook fires on every visited still-attached rung") is
    // only meaningful if we keep walking past the no-change rung.
    if (!changed && options.onAfterRefresh == null) return;
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

  // For groups + control move, capture body occupancy BEFORE the
  // `unlikeRegisters` mutation below — that mutation rewrites the
  // group's derived `.targets` cache entry that matched
  // `targetWire`, so a post-mutation read would miss it and skip
  // the children-subtree swap.
  const groupBodyIncludesTargetWire =
    movingControl &&
    sourceOperation.kind === "unitary" &&
    sourceOperation.children != null &&
    sourceOperation.targets.some((t) => t.qubit === targetWire);

  // If a different kind of register already exists, swap the control and target
  if (unlikeRegisters.find((reg) => reg.qubit === targetWire)) {
    const index = unlikeRegisters.findIndex((reg) => reg.qubit === targetWire);
    unlikeRegisters[index].qubit = sourceWire;
  }

  switch (sourceOperation.kind) {
    case "unitary":
      if (movingControl) {
        // Group + control move: dragging a control on a group
        // changes only the control's wire (body stays put).
        // Exception — if the drop wire is occupied by a body wire
        // (a child's quantum register reference on `targetWire`),
        // swap source ↔ target inside the children subtree so the
        // body wire and control trade places. The `unlikeRegisters`
        // mutation above also rewrote one entry of the group's
        // derived `.targets` cache; that gets overwritten by the
        // re-derive below, so it's harmless.
        if (sourceOperation.children != null && groupBodyIncludesTargetWire) {
          for (const col of sourceOperation.children) {
            for (const child of col.components) {
              _swapWiresInSubtree(child, sourceWire, targetWire);
            }
          }
        }
        sourceOperation.controls?.forEach((control) => {
          if (control.qubit === sourceWire) {
            control.qubit = targetWire;
          }
        });
        sourceOperation.controls = sourceOperation.controls?.sort(
          (a, b) => a.qubit - b.qubit,
        );
        // Re-derive the moved group's own `.targets` from its
        // (possibly-swapped) children. `refreshAncestorTargets`
        // (called later in `moveOperation`) walks ANCESTORS only,
        // so for the moved op itself this is the canonical spot
        // to keep the eager cache in sync.
        if (sourceOperation.children != null) {
          _refreshDerivedTargets(sourceOperation);
        }
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
 * @param sourceWire The wire the source op was "grabbed" on. Only
 *   meaningful when the source is a group or multi-target op being
 *   clone-dropped: the whole subtree is shifted by
 *   `targetWire - sourceWire` so the clone keeps its shape on the
 *   new wires (mirrors `moveOperation`'s `_moveAsUnit` path).
 *   Omit for fresh toolbox drops (single-target templates) — the
 *   single-leg rewrite below handles those.
 *
 * @returns The added operation or null if the addition was unsuccessful.
 */
const addOperation = (
  model: CircuitModel,
  sourceOperation: Operation,
  targetLocation: string,
  targetWire: number,
  insertNewColumn: boolean = false,
  sourceWire?: number,
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

  // Decide whether this clone needs the rigid unit-shift treatment.
  // Same predicate as `moveOperation` uses on its move path — a
  // group or multi-target gate must shift every register by the
  // same delta or its structure is destroyed (children stranded on
  // the old wires, multi-target gates collapsed to a single leg).
  // `movingControl` is always false here because the
  // dragController routes clone-of-a-control through addControl +
  // moveOperation, not addOperation.
  const cloneAsUnit =
    sourceWire !== undefined && _moveAsUnit(newSourceOperation, false);

  if (cloneAsUnit) {
    // Mirror `moveOperation`'s unit-shift block: refuse the clone
    // if it would push any wire below 0, then grow the model to
    // accommodate the highest wire the post-shift subtree lands on.
    const delta = targetWire - sourceWire;
    const [minOrigWire, maxOrigWire] =
      _getSubtreeMinMaxWire(newSourceOperation);
    if (minOrigWire >= 0 && minOrigWire + delta < 0) {
      return null;
    }
    model.ensureQubitCount(Math.max(targetWire, maxOrigWire + delta));
    if (delta !== 0) _shiftAllRegisters(newSourceOperation, delta);
  } else {
    // Single-leg rewrite (toolbox drop, single-target clone): the
    // op gets re-pinned to `targetWire`. Same behavior as before.
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
  }

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

  // Unit-shift clones can drop a whole subtree's worth of nested
  // measurements onto wires the model has never seen before. `_addOp`
  // only refreshes measurement lines for TOP-LEVEL measurement ops,
  // so refresh each touched wire explicitly here. Single-leg drops
  // skip this because `_addOp` already handled the only possible
  // measurement (a top-level one).
  if (cloneAsUnit) {
    const affectedMeasurementWires = new Set<number>();
    _collectMeasurementWires(newSourceOperation, affectedMeasurementWires);
    for (const wire of affectedMeasurementWires) {
      if (wire >= 0 && wire < model.qubits.length) {
        _updateMeasurementLines(model, wire);
      }
    }
  }

  // After mutating the parent group's children, the centralized
  // post-widening cleanup re-derives every ancestor's `.targets`
  // cache, then resolves any sibling-column collisions the
  // widening introduced — both at the op-itself level (redundant
  // with `_addOp`'s pre-insert check, but free under the no-op
  // path and architecturally consistent with `addControl`) and at
  // each ancestor level.
  _resolveSpanChange(
    { op: newSourceOperation, containingArray: targetOperationParent },
    destAncestorChain,
  );

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
  // Match only PURE-QUANTUM controls when checking for duplicates.
  // A classical-ref `{qubit: wireIndex, result: N}` on the same
  // wire is a different register identity (the conditional
  // dependency) and must not block adding a new quantum control.
  const existingControl = op.controls.find(
    (control) => control.qubit === wireIndex && control.result === undefined,
  );
  if (!existingControl) {
    // Capture both the op's own rung AND its ancestor chain
    // before mutating so the references survive any column
    // splices `_resolveSpanChange` may perform.
    const rungs = _findOpRungAndAncestors(model, op);
    if (rungs == null) return false;

    op.controls.push({ qubit: wireIndex });
    op.controls.sort((a, b) => a.qubit - b.qubit);
    model.ensureQubitCount(wireIndex);
    model.qubitUseCounts[wireIndex]++;

    // Adding a control on a wire outside the op's existing span
    // widens it. Run the centralized post-widening cleanup so the
    // op (and every ancestor whose span widens transitively) is
    // checked against its own column siblings. The op-itself
    // check is the one the ancestor-only cascade missed — without
    // it, a top-level `addControl` that grows the op into a
    // same-column sibling silently produced an overlapping layout.
    _resolveSpanChange(rungs.opRung, rungs.ancestorChain);
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
    // Match only PURE-QUANTUM controls. If both `{qubit: wireIndex}`
    // and `{qubit: wireIndex, result: N}` exist on the same wire,
    // "remove control" targets the quantum one; the classical-ref
    // entry is the group's conditional dependency, not a removable
    // control dot.
    const controlIndex = op.controls.findIndex(
      (control) => control.qubit === wireIndex && control.result === undefined,
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
 * Updates qubit IDs and every operation's register references —
 * including ops nested inside group `children` and the cached
 * `.targets` arrays on group ops. After the remap, refreshes
 * every group's derived `.targets` (the remap can both widen and
 * narrow group spans, so caches go stale either way) and runs the
 * overlap resolver recursively across the whole tree (widening can
 * introduce new sibling-column collisions inside groups, not just
 * at top level).
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

  // Compute the wire-index remap once and apply it to every
  // register reference in the tree — including ops nested inside
  // group children AND each group op's own cached `.targets` /
  // `.results` arrays (which are independent `Register` objects,
  // not shared references with descendants).
  const remapWire = (oldWire: number): number => {
    if (isBetween) {
      if (oldWire === sourceWire) {
        return sourceWire < targetWire ? targetWire - 1 : targetWire;
      } else if (
        sourceWire < targetWire &&
        oldWire > sourceWire &&
        oldWire < targetWire
      ) {
        return oldWire - 1;
      } else if (
        sourceWire > targetWire &&
        oldWire >= targetWire &&
        oldWire < sourceWire
      ) {
        return oldWire + 1;
      }
      return oldWire;
    } else {
      if (oldWire === sourceWire) return targetWire;
      if (oldWire === targetWire) return sourceWire;
      return oldWire;
    }
  };
  const remapRefsInGrid = (grid: ComponentGrid): void => {
    for (const column of grid) {
      for (const op of column.components) {
        getOperationRegisters(op).forEach((reg) => {
          reg.qubit = remapWire(reg.qubit);
        });
        if (op.children != null) remapRefsInGrid(op.children);
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
  };
  remapRefsInGrid(model.componentGrid);

  // Group `.targets` caches were remapped in-place above, but the
  // remap may have introduced duplicate refs (e.g. a group whose
  // `.targets` were `[{q:0}, {q:1}]` and the swap mapped both to
  // overlapping wires) or stale ordering. The deep refresh
  // re-derives each group's `.targets` from its children's caches
  // bottom-up, which is the canonical source of truth.
  _deepRefreshDerivedTargets(model.componentGrid);

  // Resolve overlaps in every column at every nesting level. The
  // top-level call from before only fixed top-level columns;
  // widening of a group's span via the remap can also introduce
  // sibling-column collisions inside that group.
  resolveOverlappingOperationsRecursive(model.componentGrid);

  model.removeTrailingUnusedQubits();
};

/**
 * Recursive variant of `resolveOverlappingOperations` — resolves
 * overlaps in every column at every nesting level of the grid.
 * Used by `moveQubit`, which can widen group spans anywhere in the
 * tree.
 */
const resolveOverlappingOperationsRecursive = (grid: ComponentGrid): void => {
  resolveOverlappingOperations(grid);
  for (const col of grid) {
    for (const op of col.components) {
      if (op.children != null) {
        resolveOverlappingOperationsRecursive(op.children);
      }
    }
  }
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
 * Get the min/max wire index of an operation's **drawn span** —
 * the wires the renderer paints a vertical connector through. Used
 * for sibling-overlap collision checks at the three sites that
 * decide whether two ops can coexist in the same column:
 * [`_addOp`](#)'s pre-insert check, [`resolveOverlappingOperations`](#)'s
 * grid sweep, and [`_resolveOverlapAfterExtend`](#)'s post-widening
 * check.
 *
 * Includes classical-control registers (`result !== undefined`),
 * because the renderer draws the connector from the gate body all
 * the way down to the producing measurement's qubit wire. A sibling
 * whose target is on `q_high` but whose classical control points
 * at a measurement on `q_low` therefore occupies every wire in
 * `[q_low, q_high]` visually — a widening op that intersects ANY
 * of those wires would collide with the drawn connector even if
 * its quantum target is on a clear wire.
 *
 * Contrast with [`getQuantumWireRange`](../utils.ts), which is the
 * right tool for "editable scope of an op" (child-drop scope,
 * shift-extend reach) but the wrong tool for collision detection —
 * those wires the classical-control connector visually occupies
 * absolutely DO collide with a sibling that overlaps them.
 *
 * Earlier versions of this helper routed through `getQuantumWireRange`
 * and missed classical-control wires; the visible symptom was a
 * widened group's expanded box drawn directly through a sibling's
 * classical-control connector with no column split — see the user's
 * "straddled between a classical-control dependency and a qubit
 * dependency" report.
 */
const _getMinMaxRegIdx = (operation: Operation): [number, number] =>
  getMinMaxRegIdx(operation);

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
  collectMeasurementConsumers,
  findAndRemoveOperations,
  moveMeasurementWithDependents,
  moveOperation,
  moveQubit,
  removeControl,
  removeMeasurementWithDependents,
  removeOperation,
  removeQubit,
  resolveOverlappingOperations,
};
