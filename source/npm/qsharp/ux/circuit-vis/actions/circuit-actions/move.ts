// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { Operation } from "../../data/circuit.js";
import { CircuitModel } from "../../data/circuitModel.js";
import { Location } from "../../data/location.js";
import { Register } from "../../data/register.js";
import { findParentArray, getOperationRegisters } from "../../utils.js";
import { addOp } from "./gridPrimitives.js";
import { collectInternalClassicalRegs } from "./classicalRefs.js";
import { refreshDerivedTargets } from "./derivedTargets.js";

/*
 * `move.ts` — the geometry of moving an operation.
 *
 * Splits a move into horizontal (`moveX`: which column/grid) and
 * vertical (`moveY`: which wires) components, plus the register-
 * shifting helpers that keep a multi-wire op's shape intact when it
 * slides as a rigid unit. The `moveOperation` orchestrator in
 * `circuitActions.ts` drives these and handles the surrounding
 * ancestor/measurement bookkeeping. Depends on `gridPrimitives`,
 * `classicalRefs`, `derivedTargets`; no DOM.
 */

/**
 * Move an operation horizontally.
 */
const moveX = (
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
  addOp(
    model,
    sourceOperation,
    targetOperationParent,
    targetLastIndex,
    insertNewColumn,
    originalOperation,
  );
};

/**
 * Move `op` as one rigid unit (shift every register by the same
 * delta) rather than rewiring just the grabbed register?
 *
 * Yes for multi-wire ops the user grabbed whole: groups (`children`),
 * SWAPs, and multi-qubit measurements — single-leg would tear them
 * apart. No for ordinary controlled gates (1 target + N controls),
 * so each leg drags independently ("rewire one leg of a CNOT").
 *
 * `movingControl` forces single-leg even on a group: dragging a
 * control rewires just that control, it doesn't slide the group.
 */
const moveAsUnit = (op: Operation, movingControl: boolean): boolean => {
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
 * Shift every register of `op`, and recursively of its children, by
 * `delta` — the rigid-unit move that keeps the gate's shape.
 *
 * Classical controls are the tricky part: a reference shifts only if
 * the measurement it depends on is also moving. Producers inside the
 * subtree shift (so their consumers do too); producers outside stay
 * put (so consumers stay anchored). We collect the inside producers
 * up front, then shift present refs and anchor absent ones.
 */
const shiftAllRegisters = (op: Operation, delta: number): void => {
  if (delta === 0) return;
  const internalProducers = collectInternalClassicalRegs(op);
  _doShift(op, delta, internalProducers);
};

/**
 * The recursive shift itself. See `shiftAllRegisters` for the
 * classical-control rationale.
 *
 * Shifts all register fields, not just `controls`: a
 * classically-conditional unitary also records the dependency in
 * `targets` (the line drawn down to the classical register box).
 * Shifting an external classical `targets` entry would point it at a
 * wire with no registers, which the renderer rejects.
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
 * Swap all references between `wireA` and `wireB` across `op`'s
 * subtree — the "drop a control onto a body wire to swap them"
 * gesture in `moveY`. Callers pass `op.children` so the group's own
 * controls/targets are left for the caller to update.
 *
 * Classical entries swap by `qubit` like quantum ones; the
 * external-producer anchoring from `_doShift` doesn't apply when
 * swapping specific wires.
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
 * Collect the wires that carry at least one measurement anywhere in `op`'s subtree, so their
 * per-wire `numResults` counters can be refreshed after a move.
 */
const collectMeasurementWires = (op: Operation, set: Set<number>): void => {
  if (op.kind === "measurement") {
    for (const q of op.qubits) set.add(q.qubit);
  }
  if (op.children) {
    for (const col of op.children) {
      for (const child of col.components) {
        collectMeasurementWires(child, set);
      }
    }
  }
};

/**
 * Move an operation vertically by changing its controls and targets.
 *
 * Pure mutator on `sourceOperation` — no grid walks, no model touches. The parent-operation
 * `targets`/`results` refresh runs at the end of `moveOperation` instead, against the post-removal
 * children grid (otherwise the parent would keep claiming the departed child's wires).
 *
 * Two semantics, picked per-op by `moveAsUnit`:
 *
 * 1. **Unit-shift** for multi-wire ops (groups, SWAP, multi-qubit
 *    measurement). The grabbed wire acts as a handle: every
 *    register on the op (and recursively every register inside
 *    `children`, with external classical refs anchored — see
 *    `shiftAllRegisters`) shifts by `targetWire - sourceWire`.
 *    The whole op slides as a rigid unit, preserving the relative
 *    arrangement of its wires.
 *
 * 2. **Single-leg rewire** for ordinary controlled-gate cases (one
 *    target + N controls). Only the grabbed register is rewritten;
 *    the other legs stay put ("rewire one leg of a CNOT").
 *
 * The "grabbed wire is the handle" model suits direct manipulation:
 * grabbing wire 4 of a group and dragging to wire 6 pins wire 4 to
 * wire 6. Richer multi-target authoring (resize, add/remove leg)
 * belongs in the Inspector, not the drag-and-drop surface.
 */
const moveY = (
  sourceOperation: Operation,
  sourceWire: number,
  targetWire: number,
  movingControl: boolean,
): void => {
  // Group / multi-target / multi-qubit ops: move the whole gate as
  // a unit (shift every register by the same delta). See
  // `moveAsUnit` for the criteria and rationale.
  if (moveAsUnit(sourceOperation, movingControl)) {
    const delta = targetWire - sourceWire;
    if (delta !== 0) shiftAllRegisters(sourceOperation, delta);
    return;
  }

  // Single-leg path (CNOT-style: rewire just one target or one
  // control leg).

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
  // `unlikeRegisters` mutation below: that mutation rewrites the
  // group's derived `.targets` entry matching `targetWire`, so a
  // post-mutation read would miss it and skip the subtree swap.
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
        // changes only the control's wire (body stays put). If the
        // drop wire is occupied by a body wire, swap source ↔ target
        // inside the children subtree so they trade places.
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
        // walks ANCESTORS only, so the moved op itself needs this.
        if (sourceOperation.children != null) {
          refreshDerivedTargets(sourceOperation);
        }
      } else {
        sourceOperation.targets = [{ qubit: targetWire }];
      }
      break;
    case "measurement":
      sourceOperation.qubits = [{ qubit: targetWire }];
      // The measurement result is updated later in the updateMeasurementLines function
      break;
    case "ket":
      sourceOperation.targets = [{ qubit: targetWire }];
      break;
  }
};

export {
  collectMeasurementWires,
  moveAsUnit,
  moveX,
  moveY,
  shiftAllRegisters,
  _swapWiresInSubtree,
};
