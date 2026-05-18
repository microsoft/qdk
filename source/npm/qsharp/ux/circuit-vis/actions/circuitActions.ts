// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { getOperationRegisters } from "../utils.js";
import { Column, ComponentGrid, Operation, Unitary } from "../data/circuit.js";
import { CircuitModel } from "../data/circuitModel.js";
import { Location } from "../data/location.js";
import { Register } from "../data/register.js";
import {
  findOperation,
  findParentArray,
  findParentOperation,
  getChildTargets,
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
  const sourceParentOperation = findParentOperation(
    model.componentGrid,
    sourceLocation,
  );

  // Capture the full ancestor chain BEFORE any mutation so the
  // empty-group cleanup at the tail of this function still has
  // valid references even after `_moveX` splices columns around.
  // (See `_collectAncestorChain` for why this has to be pre-move.)
  const ancestorChain = _collectAncestorChain(model, sourceLocation);

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

  // Refresh the source's old parent's derived `targets`/`results`
  // AFTER the removal has settled the children grid. Doing this
  // earlier (the previous behavior, inside `_moveY`) would read the
  // children grid while it still contained the original op, so the
  // parent would keep claiming the departed child's wires — visible
  // to the user as a group whose render extent stretched to wires
  // it no longer had content on.
  //
  // Note: rewriting the parent's `.targets`/`.results` here does NOT
  // adjust `qubitUseCounts`. The drift is tolerable because
  // `removeTrailingUnusedQubits` (called below) walks the tree to
  // decide what to drop — it does not consult `qubitUseCounts`.
  if (sourceParentOperation != null) {
    if (sourceParentOperation.kind === "measurement") {
      // Note: this is very confusing with measurements. Maybe the right thing to do
      // will become more apparent if we implement expandable measurements.
      sourceParentOperation.results = getChildTargets(sourceParentOperation);
    } else if (
      sourceParentOperation.kind === "unitary" ||
      sourceParentOperation.kind === "ket"
    ) {
      sourceParentOperation.targets = getChildTargets(sourceParentOperation);
    }
  }

  // Prune any ancestor groups whose children just became empty.
  // Cascades upward — removing one empty ancestor may empty its
  // grandparent. See `_pruneEmptyAncestors` for the full rationale;
  // briefly: empty groups have no semantic meaning, render
  // incorrectly (zero-wire), and trip downstream sweeps if left
  // in place.
  //
  // Runs BEFORE the measurement-line sweep and
  // `removeTrailingUnusedQubits` so those passes see the already-
  // cleaned tree.
  _pruneEmptyAncestors(ancestorChain);

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
 * Refresh the derived `targets`/`results` of `op` from its current
 * children. Mirrors the inline switch in `moveOperation`'s
 * source-parent refresh.
 */
const _refreshDerivedTargets = (op: Operation): void => {
  if (op.kind === "measurement") {
    op.results = getChildTargets(op);
  } else if (op.kind === "unitary" || op.kind === "ket") {
    op.targets = getChildTargets(op);
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
 * The walk respects an existing-refresh signal: the innermost
 * ancestor's targets are already refreshed by the call site
 * (because it's the source op's direct parent), so we skip
 * refreshing it. Any ancestor we delete demands that the NEXT
 * ancestor's targets be re-derived because it just lost a child.
 *
 * Note on `qubitUseCounts` drift. We don't adjust use-counts when
 * deleting an empty group because the group itself contributed no
 * use-count entries (the count is per leaf-op, and the empty group
 * has no leaves). `removeTrailingUnusedQubits` is the safety net.
 */
const _pruneEmptyAncestors = (chain: AncestorRung[]): void => {
  // Innermost ancestor's targets are already refreshed by the
  // caller; only ancestors we modify here need a re-derivation.
  let needsRefresh = false;
  for (const { op, containingArray } of chain) {
    if (needsRefresh) {
      _refreshDerivedTargets(op);
      needsRefresh = false;
    }
    if (!_isOperationEmpty(op)) {
      // First non-empty ancestor terminates the walk: anything
      // above this is fully populated and can't have been emptied
      // by our move.
      break;
    }
    // Splice `op` out of its containing array. Mirror the column-
    // cleanup convention from `_removeOp`: if the column that held
    // `op` is now empty, drop the column too.
    for (let colIdx = 0; colIdx < containingArray.length; colIdx++) {
      const col = containingArray[colIdx];
      const opIdx = col.components.indexOf(op);
      if (opIdx >= 0) {
        col.components.splice(opIdx, 1);
        if (col.components.length === 0) {
          containingArray.splice(colIdx, 1);
        }
        break;
      }
    }
    needsRefresh = true;
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

  _addOp(
    model,
    newSourceOperation,
    targetOperationParent,
    targetLastIndex,
    insertNewColumn,
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

  _removeOp(model, sourceOperation, sourceOperationParent);
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
    op.controls.push({ qubit: wireIndex });
    op.controls.sort((a, b) => a.qubit - b.qubit);
    model.ensureQubitCount(wireIndex);
    model.qubitUseCounts[wireIndex]++;
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
      op.controls.splice(controlIndex, 1);
      model.qubitUseCounts[wireIndex]--;
      if (wireIndex === model.qubits.length - 1) {
        model.removeTrailingUnusedQubits();
      }
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

  // Update all remaining operation references
  for (const column of model.componentGrid) {
    for (const op of column.components) {
      getOperationRegisters(op).forEach((reg) => {
        if (reg.qubit > qubitIdx) reg.qubit -= 1;
      });
    }
  }

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
const _getMinMaxRegIdx = (operation: Operation): [number, number] => {
  const qRegs: Register[] = getOperationRegisters(operation).filter(
    ({ result }) => result === undefined,
  );
  if (qRegs.length === 0) return [-1, -1];
  const qRegIdxList: number[] = qRegs.map(({ qubit }) => qubit);
  // Pad the contiguous range of registers that it covers.
  const minRegIdx: number = Math.min(...qRegIdxList);
  const maxRegIdx: number = Math.max(...qRegIdxList);

  return [minRegIdx, maxRegIdx];
};

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
  findAndRemoveOperations,
  moveOperation,
  moveQubit,
  removeControl,
  removeOperation,
  removeQubit,
  resolveOverlappingOperations,
};
