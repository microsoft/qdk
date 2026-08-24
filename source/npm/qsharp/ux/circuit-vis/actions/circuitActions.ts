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
  collectAncestorChain,
  findAncestorChainForOp,
  findOpRungAndAncestors,
} from "./circuit-actions/ancestors.js";
import {
  encodeClassicalState,
  decodeClassicalState,
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
  shouldMoveAsUnit,
  moveY,
  shiftAllRegisters,
} from "./circuit-actions/move.js";
import type { SubjectIdentity } from "./circuit-actions/classicalRefs.js";

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

type LocatedOperation = {
  operation: Operation;
  parent: ComponentGrid;
  location: string;
};

type OperationEdit = {
  operationToRemove?: LocatedOperation;
  subject?: Operation;
  targetLocation?: string;
  insertNewColumn?: boolean;
  editSubject?: (subject: Operation) => void;
  subjectIdentity?: SubjectIdentity;
};

/** Apply one complete add, move, or remove operation edit. */
const _editOperation = (
  model: CircuitModel,
  edit: OperationEdit,
): Operation | null | undefined => {
  // 1. Validate the edit shape and locate its destination before changing any state.
  const { operationToRemove, subject, targetLocation } = edit;
  if (operationToRemove == null && subject == null) return null;
  if ((subject == null) !== (targetLocation == null)) return null;

  const targetOperationParent =
    targetLocation === undefined
      ? undefined
      : findParentArray(model.componentGrid, targetLocation);
  const targetLastIndex =
    targetLocation === undefined
      ? undefined
      : Location.parse(targetLocation).last();
  if (
    subject != null &&
    (targetOperationParent == null || targetLastIndex == null)
  ) {
    return null;
  }

  if (targetOperationParent != null && targetLastIndex != null) {
    const [targetColIndex, targetOpIndex] = targetLastIndex!;
    if (targetColIndex < 0 || targetColIndex > targetOperationParent.length) {
      return null;
    }
    const targetColumnLength =
      targetOperationParent[targetColIndex]?.components.length ?? 0;
    if (targetOpIndex < 0 || targetOpIndex > targetColumnLength) return null;
  }

  // 2. Capture ancestor references before insertion or removal shifts grid locations.
  const sourceAncestors =
    operationToRemove == null
      ? []
      : collectAncestorChain(model, operationToRemove.location);
  const destinationAncestors =
    targetLocation === undefined
      ? []
      : collectAncestorChain(model, targetLocation);

  // 3. Replace positional classical references with stable identities for the structural edit.
  const classicalEncoding = encodeClassicalState(
    model.componentGrid,
    subject,
    edit.subjectIdentity,
  );

  // 4. Transform the detached subject and grow the model to contain its final wires.
  if (subject != null) {
    edit.editSubject?.(subject);
    const [, maxWire] = getSubtreeMinMaxWire(subject);
    if (maxWire >= 0) model.ensureQubitCount(maxWire);
  }

  // 5. Insert the transformed subject, then remove the requested operation.
  if (
    subject != null &&
    targetOperationParent != null &&
    targetLastIndex != null
  ) {
    addOp(
      model,
      subject,
      targetOperationParent,
      targetLastIndex,
      edit.insertNewColumn ?? false,
      operationToRemove?.operation,
    );
  }
  if (operationToRemove != null) {
    removeOp(model, operationToRemove.operation, operationToRemove.parent);
  }

  // 6. Reconcile source and destination structure after the grid reaches its final shape.
  const survivingSourceAncestors = pruneEmptyAncestors(sourceAncestors);
  refreshAncestorTargets(survivingSourceAncestors);
  if (subject != null && targetOperationParent != null) {
    resolveSpanChange(
      { op: subject, containingArray: targetOperationParent },
      destinationAncestors,
    );
  }

  // 7. Restore positional classical references and normalize the model's trailing wires.
  decodeClassicalState(model, classicalEncoding);
  model.removeTrailingUnusedQubits();
  return subject;
};

// Operation actions

/**
 * Add an operation into the circuit.
 *
 * @param sourceWire The optional anchor wire on the supplied operation. Multi-wire operations shift
 *   by `targetWire - sourceWire` so the anchor lands on the target while preserving relative wire
 *   positions. Omit for toolbox operations that should be placed on a single wire.
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
  const subject: Operation = JSON.parse(JSON.stringify(sourceOperation));
  if (subject.kind === "measurement" && subject.results.length === 0) {
    subject.results = [{ qubit: targetWire, result: 0 }];
  }
  const preserveWireLayout =
    sourceWire !== undefined && shouldMoveAsUnit(subject, false);
  if (preserveWireLayout) {
    const [minWire] = getSubtreeMinMaxWire(subject);
    if (minWire + targetWire - sourceWire < 0) return null;
  } else if (targetWire < 0) {
    return null;
  }

  return (
    _editOperation(model, {
      subject,
      targetLocation,
      insertNewColumn,
      subjectIdentity: "fork",
      editSubject: (subject) => {
        if (preserveWireLayout) {
          shiftAllRegisters(subject, targetWire - sourceWire);
        } else if (subject.kind === "measurement") {
          subject.qubits = [{ qubit: targetWire }];
          for (const result of subject.results) result.qubit = targetWire;
        } else if (subject.kind === "unitary" || subject.kind === "ket") {
          subject.targets = [{ qubit: targetWire }];
        }
      },
    }) ?? null
  );
};

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
  const sourceOperationParent = findParentArray(
    model.componentGrid,
    sourceLocation,
  );
  if (originalOperation == null || sourceOperationParent == null) return null;

  const target = Location.parse(targetLocation);
  for (const producerLocation of collectExternalProducerLocations(
    model.componentGrid,
    sourceLocation,
  )) {
    if (!Location.parse(producerLocation).inEarlierColumnThan(target)) {
      return null;
    }
  }

  // Deep clone the original op to move.
  const subject: Operation = JSON.parse(JSON.stringify(originalOperation));
  subject.dataAttributes ??= {};
  subject.dataAttributes["sqore-prev-location"] = sourceLocation;
  if (shouldMoveAsUnit(subject, movingControl)) {
    const [minWire] = getSubtreeMinMaxWire(subject);
    if (minWire + targetWire - sourceWire < 0) return null;
  } else if (targetWire < 0) {
    return null;
  }

  return (
    _editOperation(model, {
      operationToRemove: {
        operation: originalOperation,
        parent: sourceOperationParent,
        location: sourceLocation,
      },
      subject,
      targetLocation,
      insertNewColumn,
      subjectIdentity: "preserve",
      editSubject: (subject) =>
        moveY(subject, sourceWire, targetWire, movingControl),
    }) ?? null
  );
};

/** Remove an operation from the circuit. */
const removeOperation = (model: CircuitModel, sourceLocation: string) => {
  const originalOperation = findOperation(model.componentGrid, sourceLocation);
  const sourceOperationParent = findParentArray(
    model.componentGrid,
    sourceLocation,
  );
  if (originalOperation == null || sourceOperationParent == null) return null;

  return _editOperation(model, {
    operationToRemove: {
      operation: originalOperation,
      parent: sourceOperationParent,
      location: sourceLocation,
    },
  });
};

// Dependency-aware operation actions

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
  } else {
    deepRefreshDerivedTargets(model.componentGrid);
  }

  // Derived targets are refreshed above or by the batch removal; always resolve resulting overlaps.
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

// Control actions

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
 * Mirrors the structural-shape half of `shouldMoveAsUnit`.
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

// Qubit actions

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

// Private helpers

/** Find and remove operations in-place that return `true` for a predicate function. */
const _findAndRemoveOperations = (
  model: CircuitModel,
  pred: (op: Operation) => boolean,
) => {
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

export {
  addOperation,
  moveOperation,
  removeOperation,
  countStrandedConsumers,
  moveOperationWithDependents,
  removeOperationWithDependents,
  addControl,
  removeControl,
  moveQubit,
  removeQubit,
  removeQubitWithDependents,
  collectExternalProducerLocations,
  collectSubtreeConsumers,
  resolveOverlappingOperations,
  _isMultiTargetOrGroup,
};
