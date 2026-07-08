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
 * Should we move `op` as a single rigid unit (shift every register
 * by the same delta), or as a single leg (rewire just the grabbed
 * register)?
 *
 * Move as a unit when:
 *   - `op` is a group (has `children`): the user grabbed the box and
 *     expects the children to come along.
 *   - `op` has more than one target/qubit in the relevant axis (SWAP,
 *     multi-qubit measurement): single-leg would collapse it.
 *
 * Single-leg behavior covers the ordinary controlled-gate cases (one
 * target + N controls) so the user can drag the target or any one
 * control independently ("rewire one leg of a CNOT").
 *
 * `movingControl` takes precedence over the group check: a control
 * on a group is still a single leg (rewire just the control), so
 * dragging it doesn't slide the whole group.
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
 * Shift every wire-axis register of `op` — and recursively every
 * child's — by `delta`. Used when moving a multi-wire op as a rigid
 * unit so the gate keeps its shape on the new wires.
 *
 * Classical controls (registers with `result` set) need care: the
 * question isn't "is this classical?" but "is what it references also
 * moving?". A producing measurement INSIDE the moved subtree shifts
 * by the same delta, so its consumer shifts too; a producer OUTSIDE
 * stays put, so the consumer stays anchored. We first collect the
 * `(qubit, result)` tuples produced inside the subtree, then for each
 * classical control: present → shift, absent → anchor.
 */
const shiftAllRegisters = (op: Operation, delta: number): void => {
  if (delta === 0) return;
  const internalProducers = collectInternalClassicalRegs(op);
  _doShift(op, delta, internalProducers);
};

/**
 * The actual recursive shift. See `shiftAllRegisters` for the
 * classical-control rationale.
 *
 * Applies to ALL register-bearing fields, not just `controls`:
 * classically-conditional unitaries record dependencies in both
 * `controls` AND `targets` (the `targets` entries are visual extent
 * claims drawing the line down to the classical register box). A
 * naively-shifted external classical entry in `targets` would point
 * at a wire with no classical registers, which the renderer rejects.
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
 * `wireB` throughout `op`'s subtree. Used by the group +
 * `movingControl` branch in `moveY` for the "drop the control onto a
 * body wire to swap them" gesture; callers pass `op.children`
 * directly so the group's own controls/targets are left for the
 * caller to update.
 *
 * Classical-register entries get the same `qubit` swap as quantum
 * ones — here we're swapping specific wires, so the external-producer
 * "anchor" rule from `_doShift` doesn't apply.
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
 * Collect the wires that carry at least one measurement anywhere in
 * `op`'s subtree, so their per-wire `numResults` counters can be
 * refreshed after a move.
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
 * Pure mutator on `sourceOperation` — no grid walks, no model
 * touches. The parent-operation `targets`/`results` refresh runs at
 * the end of `moveOperation` instead, against the post-removal
 * children grid (otherwise the parent would keep claiming the
 * departed child's wires).
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
