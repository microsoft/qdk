// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { ComponentGrid, Operation, Unitary } from "../data/circuit.js";
import { CircuitModel } from "../data/circuitModel.js";
import { Location } from "../data/location.js";
import {
  findOperation,
  findParentArray,
  getOperationRegisters,
} from "../utils.js";
import {
  AncestorRung,
  collectAncestorChain,
  collectDestAncestorChain,
  findAncestorChainForOp,
  findOpRungAndAncestors,
} from "./circuit-actions/ancestors.js";
import {
  applyClassicalRefRemap,
  findLocationByRef,
  collectExternalProducerLocations,
  collectMeasurementConsumers,
} from "./circuit-actions/classicalRefs.js";
import {
  deepRefreshDerivedTargets,
  pruneEmptyAncestors,
  resolveSpanChange,
  refreshAncestorTargets,
} from "./circuit-actions/derivedTargets.js";
import {
  addOp,
  getSubtreeMinMaxWire,
  moveArrayElement,
  removeOp,
  updateMeasurementLines,
  resolveOverlappingOperations,
  resolveOverlappingOperationsRecursive,
} from "./circuit-actions/gridPrimitives.js";
import {
  collectMeasurementWires,
  moveAsUnit,
  moveX,
  moveY,
  shiftAllRegisters,
} from "./circuit-actions/move.js";

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
 *
 * This file is the **orchestration + public API** layer. The
 * mechanical helpers it composes live in sibling modules:
 *   - `gridPrimitives.ts` — column insert/remove, overlap, span.
 *   - `ancestors.ts` — ancestor-chain capture.
 *   - `derivedTargets.ts` — the eager `.targets` cache cascade.
 *   - `move.ts` — horizontal/vertical move geometry.
 *   - `classicalRefs.ts` — classical producer/consumer analysis.
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
  // `moveX` below may splice a fresh column into a grid that lies
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
  // valid references even after `moveX` splices columns around.
  // (See `collectAncestorChain` for why this has to be pre-move.)
  const ancestorChain = collectAncestorChain(model, sourceLocation);

  // Capture the destination's ancestor chain pre-move too, for the
  // dest-side cascade refresh below. Same reasoning as the source
  // chain: post-move, `targetLocation` may name a different op
  // (column splices, empty-prune cascades) and a string-based walk
  // would land on the wrong tree. Object references stay valid.
  // Empty array when there is no parent group (top-level drop).
  const destAncestorChain: AncestorRung[] = collectDestAncestorChain(
    model,
    targetLocation,
  );

  // Capture the destination's containing array (the grid the
  // moved op will live in directly) pre-move. Falls back to the
  // top-level grid for top-level drops. Same rationale as the
  // chain capture: the dest array reference stays valid through
  // every column splice `addOp` / dest-cascade may perform,
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
  // `updateMeasurementLines` sweep at the tail of this function).
  const affectedMeasurementWires = new Set<number>();
  collectMeasurementWires(originalOperation, affectedMeasurementWires);

  // Grow the model to accommodate the highest wire the post-move
  // op will land on. For a single-leg move this is `targetWire`.
  // For a group / multi-target move (the unit-shift path inside
  // `moveY`) every register shifts by `targetWire - sourceWire`,
  // so the high wire moves to `maxOrigWire + delta` — which can
  // be well above `targetWire` and must exist before `moveX`
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
  if (moveAsUnit(newSourceOperation, movingControl)) {
    const delta = targetWire - sourceWire;
    const [minOrigWire, maxOrigWire] = getSubtreeMinMaxWire(newSourceOperation);
    if (minOrigWire >= 0 && minOrigWire + delta < 0) {
      return null;
    }
    model.ensureQubitCount(Math.max(targetWire, maxOrigWire + delta));
  } else {
    model.ensureQubitCount(targetWire);
  }

  // Update operation's targets and controls
  moveY(newSourceOperation, sourceWire, targetWire, movingControl);

  // Capture POST-shift measurement wires too, so the refresh sweep
  // covers both the wires the measurements just left AND the wires
  // they just landed on.
  collectMeasurementWires(newSourceOperation, affectedMeasurementWires);

  // Move horizontally
  moveX(
    model,
    newSourceOperation,
    originalOperation,
    targetLocation,
    insertNewColumn,
  );

  removeOp(model, originalOperation, sourceOperationParent);

  // Source-side cleanup: prune any ancestor groups whose
  // children just collapsed to empty (cascades upward) and then
  // refresh the surviving ancestors' derived `.targets`. The two
  // sweeps used to be interleaved inside `pruneEmptyAncestors`
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
  const survivedSourceChain = pruneEmptyAncestors(ancestorChain);
  refreshAncestorTargets(survivedSourceChain);

  // Dest-side cleanup. Centralized post-widening cascade:
  //   - The newly-moved op vs its own column siblings (redundant
  //     with `addOp`'s pre-insert overlap check, but kept for
  //     architectural consistency — no-op when `addOp` already
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
  resolveSpanChange(
    {
      op: newSourceOperation,
      containingArray: destContainingArray,
    },
    destAncestorChain,
  );

  // Refresh per-wire `numResults` counters for every wire that
  // may have gained or lost a measurement. `addOp` / `removeOp`
  // only fire this for TOP-LEVEL measurements; when a measurement
  // crosses wires inside a moved group, this sweep is the only
  // thing that keeps `qubits[wire].numResults` in step with the
  // measurements actually present on that wire. Stale numResults
  // is exactly what causes the renderer to throw
  // "Classical register ID N invalid for qubit ID M with 0
  // classical register(s)" the next paint.
  for (const wire of affectedMeasurementWires) {
    if (wire >= 0 && wire < model.qubits.length) {
      updateMeasurementLines(model, wire);
    }
  }

  model.removeTrailingUnusedQubits();

  return newSourceOperation;
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
 * `moveOperation`'s tail-end `updateMeasurementLines` sweep
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
  // by the move's tail-end `updateMeasurementLines` sweep need
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
  // `updateMeasurementLines` renumbering at the tail.
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
    applyClassicalRefRemap(model.componentGrid, keyRemap);
  }

  // Consumers' visual spans may have changed (a classical-ref
  // wire moved), which can widen or narrow group `.targets`
  // caches and introduce new sibling-column collisions.
  // Re-derive bottom-up and resolve recursively, same pattern
  // `moveQubit` uses for the wire-remap case.
  deepRefreshDerivedTargets(model.componentGrid);
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
 * `removeOperation`'s tail-end `updateMeasurementLines` sweep
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
  // `updateMeasurementLines` sweep inside `removeOperation`;
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
  const newMLoc = findLocationByRef(model.componentGrid, mOp);
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
    applyClassicalRefRemap(model.componentGrid, keyRemap);
    // Visual spans on surviving classically-controlled groups
    // may have shifted; refresh + resolve overlaps (same pattern
    // as the move path).
    deepRefreshDerivedTargets(model.componentGrid);
    resolveOverlappingOperationsRecursive(model.componentGrid);
  }
};

/**
 * Add an operation into the circuit.
 *
 * @param sourceWire The wire the source op was "grabbed" on. Only
 *   meaningful when the source is a group or multi-target op being
 *   clone-dropped: the whole subtree is shifted by
 *   `targetWire - sourceWire` so the clone keeps its shape on the
 *   new wires (mirrors `moveOperation`'s `moveAsUnit` path).
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
    sourceWire !== undefined && moveAsUnit(newSourceOperation, false);

  if (cloneAsUnit) {
    // Mirror `moveOperation`'s unit-shift block: refuse the clone
    // if it would push any wire below 0, then grow the model to
    // accommodate the highest wire the post-shift subtree lands on.
    const delta = targetWire - sourceWire;
    const [minOrigWire, maxOrigWire] = getSubtreeMinMaxWire(newSourceOperation);
    if (minOrigWire >= 0 && minOrigWire + delta < 0) {
      return null;
    }
    model.ensureQubitCount(Math.max(targetWire, maxOrigWire + delta));
    if (delta !== 0) shiftAllRegisters(newSourceOperation, delta);
  } else {
    // Single-leg rewrite (toolbox drop, single-target clone): the
    // op gets re-pinned to `targetWire`. Same behavior as before.
    if (newSourceOperation.kind === "measurement") {
      newSourceOperation.qubits = [{ qubit: targetWire }];
      // The measurement result is updated later in the updateMeasurementLines function
    } else if (
      newSourceOperation.kind === "unitary" ||
      newSourceOperation.kind === "ket"
    ) {
      newSourceOperation.targets = [{ qubit: targetWire }];
    }
    model.ensureQubitCount(targetWire);
  }

  // Capture the dest ancestor chain BEFORE addOp so the rung
  // references survive any column splices `addOp` may perform.
  // Empty when targetLocation is top-level (parent is root).
  const destAncestorChain: AncestorRung[] = collectDestAncestorChain(
    model,
    targetLocation,
  );

  addOp(
    model,
    newSourceOperation,
    targetOperationParent,
    targetLastIndex,
    insertNewColumn,
  );

  // Unit-shift clones can drop a whole subtree's worth of nested
  // measurements onto wires the model has never seen before. `addOp`
  // only refreshes measurement lines for TOP-LEVEL measurement ops,
  // so refresh each touched wire explicitly here. Single-leg drops
  // skip this because `addOp` already handled the only possible
  // measurement (a top-level one).
  if (cloneAsUnit) {
    const affectedMeasurementWires = new Set<number>();
    collectMeasurementWires(newSourceOperation, affectedMeasurementWires);
    for (const wire of affectedMeasurementWires) {
      if (wire >= 0 && wire < model.qubits.length) {
        updateMeasurementLines(model, wire);
      }
    }
  }

  // After mutating the parent group's children, the centralized
  // post-widening cleanup re-derives every ancestor's `.targets`
  // cache, then resolves any sibling-column collisions the
  // widening introduced — both at the op-itself level (redundant
  // with `addOp`'s pre-insert check, but free under the no-op
  // path and architecturally consistent with `addControl`) and at
  // each ancestor level.
  resolveSpanChange(
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

  // Capture the source ancestor chain BEFORE removeOp so the rung
  // references survive the splice (and any column collapse that
  // follows when the source was the last op in its column).
  const ancestorChain = collectAncestorChain(model, sourceLocation);

  removeOp(model, sourceOperation, sourceOperationParent);

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
  deepRefreshDerivedTargets(model.componentGrid);
};

/**
 * Returns true if `op` is a multi-target unitary, multi-qubit
 * measurement, or a group (has children). The shared property:
 * the op has more than one wire-leg, so there is no single
 * canonical position at which a new quantum-control connector
 * could attach — the existing CNOT-style "one solid line from
 * top control to bottom target" rendering rule doesn't extend
 * to a body that's split across non-adjacent wires.
 *
 * Used to gate [`addControl`](#) and [`removeControl`](#): by
 * design, the editor refuses to create or destroy quantum
 * controls on such ops. For groups (any op with `children`) this
 * is a permanent design decision — groups may carry classical
 * controls only. For multi-target unitaries / measurements this
 * is a structural limitation of the rendering rule.
 *
 * Existing quantum controls in loaded `.qsc` data on a
 * multi-target unitary still render (via
 * [`_controlledGate`](../renderer/formatters/gateFormatter.ts)'s
 * ControlledUnitary branch for split multi-target unitaries).
 * Quantum controls on groups arriving from external data are
 * not rendered — there is no editor surface for them and no
 * special-case renderer logic. They can still be DRAGGED via the
 * `movingControl` leg-rewire path on shapes that already had
 * them, which is a permutation of controls already on the op
 * and doesn't introduce a new one.
 *
 * Mirrors the structural-shape half of [`moveAsUnit`](#)'s
 * predicate (the `movingControl` short-circuit there is
 * orthogonal to "does the op have multiple wire-legs?").
 */
const _isMultiTargetOrGroup = (op: Operation): boolean => {
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
 * Add a control to the specified operation on the given wire index.
 *
 * @returns True if the control was added, false if it already existed.
 */
const addControl = (
  model: CircuitModel,
  op: Unitary,
  wireIndex: number,
): boolean => {
  // Refuse on multi-target ops and groups by design (see
  // [`_isMultiTargetOrGroup`](#) for the rationale — groups never
  // carry quantum controls; multi-target bodies have no canonical
  // attachment point). Gating at the action layer ensures every
  // entry point (context menu, dropzone commit, drag flows,
  // programmatic callers) gets the same treatment without each
  // having to remember the rule.
  if (_isMultiTargetOrGroup(op)) return false;
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
    // splices `resolveSpanChange` may perform.
    const rungs = findOpRungAndAncestors(model, op);
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
    resolveSpanChange(rungs.opRung, rungs.ancestorChain);
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
  // Symmetric to [`addControl`](#): refuse on multi-target ops
  // and groups by design, so legacy controls on such ops (if
  // they arrive in external data) can be observed but not
  // destroyed through the editor surface. The `movingControl`
  // drag-leg-rewire path is permutation-only and doesn't reach
  // this function. See [`_isMultiTargetOrGroup`](#).
  if (_isMultiTargetOrGroup(op)) return false;
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
      const ancestorChain = findAncestorChainForOp(model, op);

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
    moveArrayElement(model.qubits, sourceWire, insertAt);
    moveArrayElement(model.qubitUseCounts, sourceWire, insertAt);
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
  deepRefreshDerivedTargets(model.componentGrid);

  // Resolve overlaps in every column at every nesting level. The
  // top-level call from before only fixed top-level columns;
  // widening of a group's span via the remap can also introduce
  // sibling-column collisions inside that group.
  resolveOverlappingOperationsRecursive(model.componentGrid);

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
  _isMultiTargetOrGroup,
};
