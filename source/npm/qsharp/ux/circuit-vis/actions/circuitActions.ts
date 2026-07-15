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
  moveArrayElement,
  removeOp,
  updateMeasurementLines,
  resolveOverlappingOperations,
  resolveOverlappingOperationsRecursive,
} from "./circuit-actions/gridPrimitives.js";
import {
  collectMeasurementWires,
  moveX,
  moveY,
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
 * After the move, both the source-side and dest-side ancestor chains are walked innermost-out by
 * `refreshAncestorTargets` and each still-attached ancestor's derived `.targets`/`.results` is
 * rebuilt from its post-move children, maintaining the invariant that an ancestor's `.targets` is
 * the union of its descendants' wires. The target location string is authoritative about which
 * group the op lands in.
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

  // Capture pre-move measurement wires from the live source, to refresh per-wire `numResults`
  // counters after the move (see the `updateMeasurementLines` sweep at the tail).
  const affectedMeasurementWires = new Set<number>();
  collectMeasurementWires(originalOperation, affectedMeasurementWires);

  // Grow the model to fit the wire the moved leg will land on.
  model.ensureQubitCount(targetWire);

  // Update operation's targets and controls
  moveY(newSourceOperation, sourceWire, targetWire, movingControl);

  // Capture POST-shift measurement wires too, so the sweep covers both the wires measurements left
  // and the wires they landed on.
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

  // Refresh per-wire `numResults` counters for every wire that may have gained or lost a
  // measurement. `addOp` / `removeOp` only fire this for TOP-LEVEL measurements; a measurement
  // crossing wires inside a moved group is only kept in step here.
  for (const wire of affectedMeasurementWires) {
    if (wire >= 0 && wire < model.qubits.length) {
      updateMeasurementLines(model, wire);
    }
  }

  model.removeTrailingUnusedQubits();

  return newSourceOperation;
};

/**
 * Move a measurement that has downstream classical consumers, propagating the effects to those
 * consumers.
 *
 * Wraps `moveOperation` with the bookkeeping to keep the classical producer→consumer graph
 * consistent. The caller (the editor's prompt layer) is expected to have already:
 *   1. Called `collectMeasurementConsumers` on the M.
 *   2. Partitioned the result against `targetLocation` by
 *      [`Location.inEarlierColumnThan`](../data/location.ts) into survivors (their classical refs
 *      get remapped) and invalidated consumers (passed as `invalidatedConsumers`, cascade-deleted).
 *   3. Confirmed the cascade with the user.
 *
 * Wire-remap detail: `moveOperation`'s tail-end `updateMeasurementLines` sweep renumbers result
 * indices on every affected wire, so we snapshot each measurement's pre-move `(qubit, result)` keys
 * and compare with post-move keys to build a complete remap (also catching consumers of OTHER Ms on
 * the renumbered wires). The moved M becomes a new object reference, so its pre-move keys are
 * captured separately and paired positionally.
 *
 * Ordering: move first, then cascade-delete — the pre-move `targetLocation` is still valid against
 * the unmodified grid, and cascade-delete uses object-reference predicates that survive the move's
 * column splices.
 *
 * Returns the moved M op, or `null` if `moveOperation` refused it.
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

  // Snapshot every M's pre-move (qubit, result) keys by object identity. Other Ms renumbered by the
  // tail-end `updateMeasurementLines` sweep need their consumers updated too.
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

  // The moving M's pre-keys captured SEPARATELY: it becomes a new object post-move (deep-cloned),
  // so the by-ref map won't have it.
  const movedMPreKeys = preMoveKeysByRef.get(mOp) ?? [];

  // Move M. The standard path handles its wire change, column placement, and the global
  // `updateMeasurementLines` renumbering.
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

  // Cascade-delete invalidated consumers AFTER the move. The predicate matches on object identity,
  // so shifted locations don't matter.
  const invalidatedSet = new Set(invalidatedConsumers);
  if (invalidatedSet.size > 0) {
    _findAndRemoveOperations(model, (op) => invalidatedSet.has(op));
  }

  // Build the (oldQubit, oldResult) → (newQubit, newResult) remap by pairing pre/post snapshots
  // positionally per M: the moved M from `movedMPreKeys` → `movedM.results`, every other M from
  // `preMoveKeysByRef` → its still-live op.
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

  // Apply the remap to every classical ref in the grid. Walking the whole grid catches consumers of
  // OTHER Ms bumped by the renumber.
  if (keyRemap.size > 0) {
    applyClassicalRefRemap(model.componentGrid, keyRemap);
  }

  // Consumers' visual spans may have changed, widening or narrowing group `.targets` caches and
  // introducing new collisions. Re-derive bottom-up and resolve recursively (same pattern as
  // `moveQubit`).
  deepRefreshDerivedTargets(model.componentGrid);
  resolveOverlappingOperationsRecursive(model.componentGrid);

  return movedM;
};

/**
 * Remove a measurement and cascade-delete every op that depends on its classical outputs.
 *
 * Same handoff contract as `moveMeasurementWithDependents`: the prompt layer collects consumers,
 * confirms with the user, then calls this with the consumer set.
 *
 * Cascade-delete consumers FIRST so `removeOperation`'s ancestor refresh runs against a grid with
 * no dangling classical refs to the deleted M (whose location may shift in the cascade, so we look
 * it back up by object reference).
 *
 * Result-index propagation: `removeOperation`'s tail-end `updateMeasurementLines` sweep renumbers
 * per-wire result indices to close the gap. If OTHER Ms share that wire, their consumers keep stale
 * keys, so we snapshot every surviving M's pre-removal keys by identity, remove, then build and
 * apply a pre/post remap (same mechanism as the move path). The deleted M is excluded.
 */
const removeMeasurementWithDependents = (
  model: CircuitModel,
  mLocation: string,
  consumers: Operation[],
): void => {
  const mOp = findOperation(model.componentGrid, mLocation);
  if (mOp == null) return;

  // Snapshot every OTHER M's pre-removal (qubit, result) keys by identity. The deleted M is
  // excluded; surviving Ms on the same wire(s) get renumbered by `removeOperation`'s sweep and
  // their consumers need a matching remap.
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

  // Cascade-delete the consumers. The predicate matches on object identity, so location-string
  // drift doesn't matter.
  if (consumers.length > 0) {
    const consumerSet = new Set(consumers);
    _findAndRemoveOperations(model, (op) => consumerSet.has(op));
  }

  // M's location may have shifted in the cascade; re-derive by ref.
  const newMLoc = findLocationByRef(model.componentGrid, mOp);
  if (newMLoc != null) {
    removeOperation(model, newMLoc);
  }

  // Build the remap by pairing pre/post snapshots positionally per surviving M. An M
  // cascade-deleted with a consumer group is dropped: it isn't visited in the post-removal walk.
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
    // Surviving classically-controlled groups' spans may have shifted; refresh + resolve overlaps
    // (same as the move path).
    deepRefreshDerivedTargets(model.componentGrid);
    resolveOverlappingOperationsRecursive(model.componentGrid);
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

  // Single-leg rewrite (toolbox drop, single-target clone): re-pin the op to `targetWire`.
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

  // Capture the dest ancestor chain BEFORE addOp so the rung references survive any column splices.
  // Empty when top-level.
  const destAncestorChain: AncestorRung[] = collectAncestorChain(
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

  // After mutating the parent group's children, the centralized post-widening cleanup re-derives
  // every ancestor's `.targets` and resolves any sibling-column collisions the widening introduced.
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

  // Capture the source ancestor chain BEFORE removeOp so the rung references survive the splice
  // (and any column collapse).
  const ancestorChain = collectAncestorChain(model, sourceLocation);

  removeOp(model, sourceOperation, sourceOperationParent);

  // Re-derive the parent's `.targets` (and every ancestor above) from the surviving children.
  // Narrowing-only: shrinking a span can't introduce new sibling collisions, so no resolver hook.
  refreshAncestorTargets(ancestorChain);

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
  collectMeasurementConsumers,
  moveMeasurementWithDependents,
  moveOperation,
  moveQubit,
  removeControl,
  removeMeasurementWithDependents,
  removeOperation,
  removeQubit,
  removeQubitWithDependents,
  resolveOverlappingOperations,
  _isMultiTargetOrGroup,
};
