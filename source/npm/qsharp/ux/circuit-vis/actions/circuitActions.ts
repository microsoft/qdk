// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { ComponentGrid, Operation, Unitary } from "../data/circuit.js";
import { CircuitModel } from "../data/circuitModel.js";
import { Location } from "../data/location.js";
import {
  findOperation,
  findParentArray,
  getMinMaxRegIdx,
  getOperationRegisters,
} from "../utils.js";
import {
  AncestorRung,
  collectAncestorChain,
  findAncestorChainForOp,
  findOpRungAndAncestors,
} from "./circuit-actions/ancestors.js";
import {
  encodeClassicalResultTokens,
  decodeClassicalResultTokens,
  resequenceClassicalResults,
  findLocationByRef,
  collectExternalProducerLocations,
  collectSubtreeConsumers,
  findInvalidatedConsumers,
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
  resolveOverlappingOperations,
  resolveOverlappingOperationsRecursive,
  willInsertNewColumn,
  _isClassicallyControlled,
} from "./circuit-actions/gridPrimitives.js";
import {
  collectMeasurementWires,
  moveAsUnit,
  moveX,
  moveY,
  shiftAllRegisters,
} from "./circuit-actions/move.js";

/*
 * `circuitActions.ts` — the Action layer in the Data / Action / View architecture.
 *
 * Each exported function takes a `CircuitModel` first and mutates it in place — no DOM, interaction
 * state, or rendering. They return the new/affected `Operation` or a `boolean` status, and (being
 * pure data mutations) can be tested directly against a freshly built `CircuitModel` with no JSDOM.
 *
 * This is the orchestration + public API layer; the mechanical helpers live in sibling modules:
 * `gridPrimitives` (column insert/remove, overlap, span), `ancestors` (chain capture),
 * `derivedTargets` (the eager `.targets` cascade), `move` (move geometry), `classicalRefs`
 * (producer/consumer analysis).
 */

/**
 * Move an operation in the circuit.
 *
 * After the move, each side's ancestor chain has its derived `.targets`/`.results` rebuilt from its
 * post-move children, maintaining the invariant that an ancestor's `.targets` is the union of its
 * descendants' wires: the source-side survivors via `refreshAncestorTargets`, the dest-side chain
 * via `resolveSpanChange` (which also resolves any collisions the widening introduced). The target
 * location string is authoritative about which group the op lands in.
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

  // Resolve source-side parent references BEFORE any mutation: `moveX` may splice a fresh column
  // into a grid on the source's path, invalidating its location string. The array reference stays
  // valid as its contents shift.
  const sourceOperationParent = findParentArray(
    model.componentGrid,
    sourceLocation,
  );
  if (sourceOperationParent == null) return null;

  // Capture the source ancestor chain BEFORE any mutation so the empty-group cleanup at the tail
  // keeps valid references after `moveX` splices columns.
  const ancestorChain = collectAncestorChain(model, sourceLocation);

  // Dest ancestor chain, captured pre-move for the dest-side cascade below. Empty for a top-level
  // drop.
  const destAncestorChain: AncestorRung[] = collectAncestorChain(
    model,
    targetLocation,
  );

  // Dest containing array (the grid the moved op lives in directly), captured pre-move; falls back
  // to the top-level grid.
  const destContainingArray: ComponentGrid =
    findParentArray(model.componentGrid, targetLocation) ?? model.componentGrid;

  // Safety net: refuse the move if it would place the source before one of its external
  // classical-register producers in document order. The dropzone filter in
  // [`DragController`](../editor/controllers/dragController.ts) hides invalid dropzones at
  // drag-start; this catches any path that bypasses it. Compares PRE-mutation locations via
  // [`Location.inEarlierColumnThan`](../data/location.ts).
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

  // Stamp the clone with a one-shot "previous location" marker so
  // [`Sqore.rebaseViewState`](../sqore.ts) can transfer the user's expand/collapse state across the
  // move. The JSON deep-clone below breaks object identity, so the identity lookup would otherwise
  // miss and drop the ViewState entry. The stamp is consumed on the next rebase, so it never leaks
  // into the rendered SVG.
  if (newSourceOperation.dataAttributes == null) {
    newSourceOperation.dataAttributes = {};
  }
  newSourceOperation.dataAttributes["sqore-prev-location"] = sourceLocation;

  // Grow the model to fit the highest wire the moved op will land
  // on. For a single-leg move that's `targetWire`; for a unit-shift
  // every register shifts by `targetWire - sourceWire`, so the high
  // wire moves to `maxOrigWire + delta`, which can exceed it.
  // Refuse the move if a unit-shift would push any wire below 0
  // (the model has no negative wires); the drop silently no-ops.
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

  // Before shifting anything, give every classical register on the move's affected wires a stable
  // identity (a unique negative token) so producer→consumer links survive the wire-shift and the
  // result-renumber. `decodeClassicalResultTokens` rebuilds real indices at the tail. Applies to
  // every move — groups, bare measurements, and plain gates alike — and self-guards to a no-op when
  // the moved subtree carries no measurements. `originalOperation` is still in the grid here and is
  // skipped; the clone is walked directly. Returns the affected wires (source + landing) so decode
  // can refresh their `numResults` without a second measurement-wire sweep.
  const affectedMeasurementWires = encodeClassicalResultTokens(
    model.componentGrid,
    originalOperation,
    newSourceOperation,
    targetWire - sourceWire,
  );

  // Update operation's targets and controls
  moveY(newSourceOperation, sourceWire, targetWire, movingControl);

  // Move horizontally
  moveX(
    model,
    newSourceOperation,
    originalOperation,
    targetLocation,
    insertNewColumn,
  );

  removeOp(model, originalOperation, sourceOperationParent);

  // Source-side cleanup: prune any ancestor groups whose children just collapsed to empty (cascades
  // upward), then refresh the surviving ancestors' derived `.targets`. Prune before refresh:
  // `_isOperationEmpty` reads `children`, so refreshing a soon-to-be-deleted rung is wasted work.
  const survivedSourceChain = pruneEmptyAncestors(ancestorChain);
  refreshAncestorTargets(survivedSourceChain);

  // Dest-side cleanup. Centralized post-widening cascade: the newly-moved op vs its own column
  // siblings, plus every dest ancestor whose `.targets` no longer encloses its child's wire span
  // (with the collision resolver firing on each). Always-on because the target location is
  // authoritative; no-op for a top-level drop or when every dest ancestor was pruned.
  resolveSpanChange(
    {
      op: newSourceOperation,
      containingArray: destContainingArray,
    },
    destAncestorChain,
  );

  // Rebuild real, contiguous result indices on the tokenized wires, repoint every tokenized consumer
  // at its producer's final `(qubit, result)`, and refresh each affected wire's `numResults` counter.
  // Runs after span resolution (document order settled). Self-guards to a no-op when the move
  // tokenized nothing. This is the sole authority for classical-result bookkeeping in the move path.
  decodeClassicalResultTokens(model, affectedMeasurementWires);

  model.removeTrailingUnusedQubits();

  return newSourceOperation;
};

/**
 * Count the classical consumers a move would strand, without moving. Treats the moved subtree as
 * one producer landing at a single column: a consumer survives iff that column is strictly earlier.
 * Prompt-only; the commit path re-derives the exact set on the real post-move grid, so both agree.
 */
const countStrandedConsumers = (
  model: CircuitModel,
  sourceLocation: string,
  targetLocation: string,
  sourceWire: number,
  targetWire: number,
  movingControl: boolean,
  insertNewColumn: boolean,
): number => {
  const grid = model.componentGrid;

  // 1. Subtree + its consumers, else nothing to strand.
  const subtree = findOperation(grid, sourceLocation);
  if (subtree == null) return 0;
  const consumers = collectSubtreeConsumers(grid, sourceLocation);
  if (consumers.length === 0) return 0;

  // 2. Target column (never the root scope).
  const targetLoc = Location.parse(targetLocation);
  const last = targetLoc.last();
  if (last == null) return 0;

  // 3. Bail if the move would be refused (lands before an external producer).
  for (const pLoc of collectExternalProducerLocations(grid, sourceLocation)) {
    if (!Location.parse(pLoc).inEarlierColumnThan(targetLoc)) return 0;
  }

  // 4. Post-vertical-move span: clone one op, run the real moveY (no grid touch).
  const [colIndex, opIndex] = last;
  const movedClone: Operation = JSON.parse(JSON.stringify(subtree));
  moveY(movedClone, sourceWire, targetWire, movingControl);

  // 5. Replay addOp's injection decision -> landing column (colIndex - 0.5 if injected).
  const parentArray = findParentArray(grid, targetLocation) ?? grid;
  const inject = willInsertNewColumn(
    parentArray[colIndex],
    getMinMaxRegIdx(movedClone),
    _isClassicallyControlled(subtree),
    insertNewColumn,
    subtree,
  );
  const landing = targetLoc
    .parent()
    .child(inject ? colIndex - 0.5 : colIndex, opIndex);

  // 6. Stranded = consumers whose column isn't strictly after the landing column.
  let stranded = 0;
  for (const c of consumers) {
    if (!landing.inEarlierColumnThan(Location.parse(c.location))) stranded++;
  }
  return stranded;
};

/**
 * Move an operation, then cascade-delete the classical consumers it strands. `moveOperation`'s
 * token pass already repoints surviving consumers; this self-detects the stranded ones via
 * `findInvalidatedConsumers` on the post-move grid and removes them, then refreshes derived targets
 * and resolves overlaps.
 *
 * Kind-agnostic and self-detecting (caller hands in no invalidated set), so a group stranding
 * several consumers deletes them all in one pass. Callers that must confirm first use
 * `countStrandedConsumers`. Returns the moved op, or `null` if the move was refused.
 */
const moveOperationWithDependents = (
  model: CircuitModel,
  sourceLocation: string,
  targetLocation: string,
  sourceWire: number,
  targetWire: number,
  movingControl: boolean,
  insertNewColumn: boolean,
): Operation | null => {
  const moved = moveOperation(
    model,
    sourceLocation,
    targetLocation,
    sourceWire,
    targetWire,
    movingControl,
    insertNewColumn,
  );
  if (moved == null) return null;

  // Cascade-delete the stranded consumers, matched by identity so delete-driven location drift
  // doesn't matter.
  const stranded = findInvalidatedConsumers(model.componentGrid);
  if (stranded.length > 0) {
    const doomed = new Set(stranded.map((c) => c.op));
    _findAndRemoveOperations(model, (op) => doomed.has(op));
  }

  // Spans may have changed; re-derive targets bottom-up and resolve overlaps (same as `moveQubit`).
  deepRefreshDerivedTargets(model.componentGrid);
  resolveOverlappingOperationsRecursive(model.componentGrid);

  return moved;
};

/**
 * Remove an operation and cascade-delete every consumer of a classical output produced in its
 * subtree. The prompt layer collects `consumers` up front (via `collectSubtreeConsumers`) and
 * passes them in — detection must run BEFORE the remove because `removeOperation` renumbers
 * survivors by key, which can reuse a vacated index.
 *
 * Deletes the consumers first (matched by identity), then removes the subtree (re-derived by ref,
 * since the cascade may shift its location), then refreshes derived targets and resolves overlaps.
 */
const removeOperationWithDependents = (
  model: CircuitModel,
  location: string,
  consumers: Operation[],
): void => {
  const op = findOperation(model.componentGrid, location);
  if (op == null) return;

  // Cascade-delete the consumers, matched by identity so location drift doesn't matter.
  if (consumers.length > 0) {
    const consumerSet = new Set(consumers);
    _findAndRemoveOperations(model, (op) => consumerSet.has(op));
  }

  // Location may have shifted in the cascade; re-derive by ref. `removeOperation` renumbers the
  // surviving producers on the affected wire(s) and repoints their consumers.
  const newLoc = findLocationByRef(model.componentGrid, op);
  if (newLoc != null) {
    removeOperation(model, newLoc);
  }

  // Refresh derived targets and resolve overlaps (same as the move path).
  deepRefreshDerivedTargets(model.componentGrid);
  resolveOverlappingOperationsRecursive(model.componentGrid);
};

/**
 * Add an operation into the circuit.
 *
 * @param sourceWire The wire the source op was "grabbed" on. Only
 *   meaningful when clone-dropping a group or multi-target op: the
 *   subtree shifts by `targetWire - sourceWire` to keep its shape
 *   (mirrors `moveOperation`'s `moveAsUnit` path). Omit for fresh
 *   toolbox drops, which take the single-leg rewrite below.
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

  // Reject an out-of-range location on either axis.
  const [targetColIndex, targetOpIndex] = targetLastIndex;
  if (targetColIndex < 0 || targetColIndex > targetOperationParent.length) {
    return null;
  }
  // A brand-new trailing column doesn't exist yet, so its length is 0 — only op index 0 is valid.
  const targetColumnLength =
    targetOperationParent[targetColIndex]?.components.length ?? 0;
  if (targetOpIndex < 0 || targetOpIndex > targetColumnLength) {
    return null;
  }

  // Create a deep copy of the source operation
  const newSourceOperation: Operation = JSON.parse(
    JSON.stringify(sourceOperation),
  );

  // Decide whether this clone needs the rigid unit-shift treatment
  // (same predicate as `moveOperation`'s move path). `movingControl`
  // is always false here — clone-of-a-control routes through
  // addControl + moveOperation, not addOperation.
  const cloneAsUnit =
    sourceWire !== undefined && moveAsUnit(newSourceOperation, false);

  if (cloneAsUnit) {
    // Mirror `moveOperation`'s unit-shift block: refuse if it would
    // push any wire below 0, then grow the model to fit.
    const delta = targetWire - sourceWire;
    const [minOrigWire, maxOrigWire] = getSubtreeMinMaxWire(newSourceOperation);
    if (minOrigWire >= 0 && minOrigWire + delta < 0) {
      return null;
    }
    model.ensureQubitCount(Math.max(targetWire, maxOrigWire + delta));
    if (delta !== 0) shiftAllRegisters(newSourceOperation, delta);
  } else {
    // Single-leg rewrite (toolbox drop, single-target clone): re-pin
    // the op to `targetWire`.
    if (newSourceOperation.kind === "measurement") {
      newSourceOperation.qubits = [{ qubit: targetWire }];
      // Stamp the new producer with a sentinel result index (`-1`): `resequenceClassicalResults`
      // below assigns its real position on `targetWire`. The sentinel shares no `(qubit, result)`
      // key with any existing producer, so it can't collide with one during the consumer remap.
      newSourceOperation.results = [{ qubit: targetWire, result: -1 }];
    } else if (
      newSourceOperation.kind === "unitary" ||
      newSourceOperation.kind === "ket"
    ) {
      newSourceOperation.targets = [{ qubit: targetWire }];
    }
    model.ensureQubitCount(targetWire);
  }

  // Capture the dest ancestor chain BEFORE addOp so the rung references survive any column splices.
  // Empty when top-level.
  const destAncestorChain: AncestorRung[] = collectAncestorChain(
    model,
    targetLocation,
  );

  // Collect the wires this op carries a measurement on; used to reindex classical results below. A
  // no-op for a non-measurement op (nothing collected).
  const affectedMeasurementWires = new Set<number>();
  collectMeasurementWires(newSourceOperation, affectedMeasurementWires);

  addOp(
    model,
    newSourceOperation,
    targetOperationParent,
    targetLastIndex,
    insertNewColumn,
  );

  // After mutating the parent group's children, the centralized post-widening cleanup re-derives
  // every ancestor's `.targets` and resolves any sibling-column collisions the widening introduced.
  resolveSpanChange(
    { op: newSourceOperation, containingArray: targetOperationParent },
    destAncestorChain,
  );

  // Resequence result indices on the affected wires, carry each downstream consumer to its
  // producer's new slot, and refresh each wire's `numResults`. `addOp` is a structural primitive
  // with no measurement bookkeeping of its own; this action owns it for adds.
  resequenceClassicalResults(model, affectedMeasurementWires);

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

  // Capture the source ancestor chain BEFORE removeOp so the rung references survive the splice
  // (and any column collapse).
  const ancestorChain = collectAncestorChain(model, sourceLocation);

  // Capture the removed op's measurement wires BEFORE removal so the surviving Ms on those wires can
  // be renumbered afterward (and their consumers carried, `numResults` closed up).
  const affectedMeasurementWires = new Set<number>();
  collectMeasurementWires(sourceOperation, affectedMeasurementWires);

  removeOp(model, sourceOperation, sourceOperationParent);

  // Resequence the surviving producers on those wires in document order, carry each of their
  // consumers to the producer's new slot, and refresh each wire's `numResults`.
  resequenceClassicalResults(model, affectedMeasurementWires);

  // Re-derive the parent's `.targets` (and every ancestor above) from the surviving children.
  // Narrowing-only: shrinking a span can't introduce new sibling collisions, so no resolver hook.
  const survivedChain = pruneEmptyAncestors(ancestorChain);
  refreshAncestorTargets(survivedChain);

  model.removeTrailingUnusedQubits();
};

/**
 * Find and remove operations in-place that return `true` for a predicate function.
 */
const _findAndRemoveOperations = (
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

  // Batch removal may have stripped ops from many ancestor chains, so re-derive every group's cache
  // in one bottom-up sweep.
  deepRefreshDerivedTargets(model.componentGrid);
};

/**
 * Returns true if `op` is a multi-target unitary, multi-qubit measurement, or a group — i.e. an op
 * with more than one wire-leg, with no single canonical position to attach a quantum-control
 * connector.
 *
 * Gates `addControl` and `removeControl`: the editor refuses to create or destroy quantum controls
 * on such ops. Groups carry classical controls only; for multi-target ops it's a rendering-rule
 * limitation. Existing quantum controls in loaded `.qsc` data still render and can be dragged (the
 * `movingControl` path permutes existing controls rather than adding one).
 *
 * Mirrors the structural-shape half of `moveAsUnit`.
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
  // Refuse on multi-target ops and groups by design (see `_isMultiTargetOrGroup`). Gating here
  // covers every entry point uniformly.
  if (_isMultiTargetOrGroup(op)) return false;
  if (!op.controls) {
    op.controls = [];
  }
  // Match only PURE-QUANTUM controls. A classical-ref on the same wire is a different register
  // identity and must not block adding a new quantum control.
  const existingControl = op.controls.find(
    (control) => control.qubit === wireIndex && control.result === undefined,
  );
  if (!existingControl) {
    // Capture the op's rung and ancestor chain before mutating so the references survive any column
    // splices.
    const rungs = findOpRungAndAncestors(model, op);
    if (rungs == null) return false;

    op.controls.push({ qubit: wireIndex });
    op.controls.sort((a, b) => a.qubit - b.qubit);
    model.ensureQubitCount(wireIndex);
    model.qubitUseCounts[wireIndex]++;

    // Adding a control outside the op's span widens it. Run the centralized post-widening cleanup
    // so the op (and every ancestor that widens transitively) is checked against its siblings.
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
  // Symmetric to `addControl`: refuse on multi-target ops and groups by design. The `movingControl`
  // drag path is permutation-only and doesn't reach here. See `_isMultiTargetOrGroup`.
  if (_isMultiTargetOrGroup(op)) return false;
  if (op.controls) {
    // Match only PURE-QUANTUM controls; a classical-ref entry on the same wire is the group's
    // conditional dependency, not a removable control dot.
    const controlIndex = op.controls.findIndex(
      (control) => control.qubit === wireIndex && control.result === undefined,
    );
    if (controlIndex !== -1) {
      // Capture ancestors before mutating, for consistency with the other mutators (narrowing can't
      // trigger column splices).
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
 *   - `isBetween: true`  — insert before `targetWire`.
 *   - `isBetween: false` — swap with `targetWire`.
 *
 * Updates qubit IDs and every register reference (including ops nested in group `children` and the
 * cached `.targets` on groups), then refreshes every group's derived `.targets` and runs the
 * overlap resolver recursively (the remap can both widen and narrow spans). No-op if `sourceWire
 * === targetWire` or either is null.
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

  // Compute the wire-index remap once and apply it to every register reference in the tree —
  // including ops nested in group children and each group's own cached `.targets` / `.results`
  // (independent `Register` objects, not shared with descendants).
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

  // Group `.targets` caches were remapped in-place above, but that may have introduced duplicate
  // refs or stale ordering. The deep refresh re-derives each group's `.targets` from its children
  // bottom-up, the canonical source of truth.
  deepRefreshDerivedTargets(model.componentGrid);

  // Resolve overlaps at every nesting level: widening a group's span via the remap can introduce
  // collisions inside that group too.
  resolveOverlappingOperationsRecursive(model.componentGrid);

  model.removeTrailingUnusedQubits();
};

/**
 * Remove a qubit line at `qubitIdx`. Caller is responsible for asking the user to confirm if the
 * wire still has operations on it; this function only does the data mutation.
 *
 * Decrements all references on higher-numbered wires by 1 (since their indices shift down) and
 * renumbers qubit ids to match. Operations that touched `qubitIdx` are **not** removed by this call
 * — use `removeQubitWithDependents` if you want the ops on the wire stripped too.
 */
const removeQubit = (model: CircuitModel, qubitIdx: number): void => {
  model.qubits.splice(qubitIdx, 1);
  model.qubitUseCounts.splice(qubitIdx, 1);
  model.removeTrailingUnusedQubits();

  // Update all references throughout the tree — including ops nested in groups and the eager
  // `.targets` / `.results` caches on those groups. Walking recursively keeps child refs and cached
  // refs in lockstep, so the uniform shift preserves cache coherence.
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
 * Remove a qubit line at `qubitIdx` together with every operation that touches it. Counterpart to
 * the measurement `*WithDependents` actions: strips every op with a register on `qubitIdx`, then
 * drops the wire and renumbers the higher wires down.
 *
 * The strip must run BEFORE `removeQubit`, which shifts higher wires down by one and would
 * otherwise invalidate `qubitIdx` mid-cascade.
 */
const removeQubitWithDependents = (
  model: CircuitModel,
  qubitIdx: number,
): void => {
  _findAndRemoveOperations(model, (op) =>
    getOperationRegisters(op).some((reg) => reg.qubit === qubitIdx),
  );
  removeQubit(model, qubitIdx);
};

export {
  addControl,
  addOperation,
  collectExternalProducerLocations,
  collectSubtreeConsumers,
  countStrandedConsumers,
  moveOperationWithDependents,
  moveOperation,
  moveQubit,
  removeControl,
  removeOperationWithDependents,
  removeOperation,
  removeQubit,
  removeQubitWithDependents,
  resolveOverlappingOperations,
  _isMultiTargetOrGroup,
};
