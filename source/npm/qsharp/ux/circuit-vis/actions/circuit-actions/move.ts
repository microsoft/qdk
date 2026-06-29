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
 * Splits a move into its horizontal (`moveX`: which column/grid the
 * op files into) and vertical (`moveY`: which wires its registers
 * land on) components, plus the register-shifting helpers that keep a
 * multi-wire op's shape intact when it slides as a rigid unit. The
 * `moveOperation` orchestrator in `circuitActions.ts` drives these in
 * sequence and handles the surrounding ancestor/measurement
 * bookkeeping. Depends on `gridPrimitives`, `classicalRefs`, and
 * `derivedTargets`; no DOM.
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
const shiftAllRegisters = (op: Operation, delta: number): void => {
  if (delta === 0) return;
  const internalProducers = new Set<string>();
  collectInternalClassicalRegs(op, internalProducers);
  _doShift(op, delta, internalProducers);
};

/**
 * The actual recursive shift. See `shiftAllRegisters` for the
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
 * `moveY` to implement the "drop the control onto a body wire to
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
 * touches. The parent-operation `targets`/`results` refresh that
 * used to live at the tail of this function is now done at the end
 * of `moveOperation` instead, so it runs against the post-removal
 * children grid (otherwise the parent would keep claiming the
 * departed child's wires).
 *
 * Two semantics, picked per-op by [`moveAsUnit`](#):
 *
 * 1. **Unit-shift** for multi-wire ops (groups, SWAP, multi-qubit
 *    measurement). The grabbed wire acts as a handle: every
 *    register on the op (and recursively every register inside
 *    `children`, with external classical refs anchored — see
 *    [`shiftAllRegisters`](#)) shifts by `targetWire - sourceWire`.
 *    The whole op slides as a rigid unit, preserving the relative
 *    arrangement of its wires.
 *
 * 2. **Single-leg rewire** for ordinary controlled-gate cases (one
 *    target + N controls). Only the grabbed register is rewritten;
 *    the other legs stay put. This is the established "rewire one
 *    leg of a CNOT" interaction.
 *
 * **Design decision (D3 in [CIRCUIT_EDITOR_TODO.md](../../CIRCUIT_EDITOR_TODO.md)).**
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
