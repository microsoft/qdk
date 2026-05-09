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
 * three-layer architecture (Data / Action / View — see
 * [CIRCUIT_EDITOR_TODO.md](CIRCUIT_EDITOR_TODO.md)).
 *
 * Each exported function takes a `CircuitModel` (Data layer) as its
 * first argument and mutates it in place. **No DOM. No interaction
 * state. No rendering.** Functions return either the new/affected
 * `Operation` (when the caller needs a handle to it) or a `boolean`
 * status flag — the choice matches each function's pre-R3 contract,
 * to minimize churn in the UI code that calls them.
 *
 * Direct unit-testability of this module is the main R3 win — Actions
 * can be exercised against a freshly-constructed `CircuitModel` with
 * no JSDOM and no `CircuitEvents` stub.
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

  // Create a deep copy of the source operation
  const newSourceOperation: Operation = JSON.parse(
    JSON.stringify(originalOperation),
  );

  model.ensureQubitCount(targetWire);

  // Update operation's targets and controls
  _moveY(
    model,
    newSourceOperation,
    sourceLocation,
    sourceWire,
    targetWire,
    movingControl,
  );

  // Move horizontally
  _moveX(
    model,
    newSourceOperation,
    originalOperation,
    targetLocation,
    insertNewColumn,
  );

  const sourceOperationParent = findParentArray(
    model.componentGrid,
    sourceLocation,
  );
  if (sourceOperationParent == null) return null;
  _removeOp(model, originalOperation, sourceOperationParent);
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
 * Move an operation vertically by changing its controls and targets.
 */
const _moveY = (
  model: CircuitModel,
  sourceOperation: Operation,
  sourceLocation: string,
  sourceWire: number,
  targetWire: number,
  movingControl: boolean,
): void => {
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

  // Update parent operation targets
  const parentOperation = findParentOperation(
    model.componentGrid,
    sourceLocation,
  );
  if (parentOperation) {
    if (parentOperation.kind === "measurement") {
      // Note: this is very confusing with measurements. Maybe the right thing to do
      // will become more apparent if we implement expandable measurements.
      parentOperation.results = getChildTargets(parentOperation);
    } else if (
      parentOperation.kind === "unitary" ||
      parentOperation.kind === "ket"
    ) {
      parentOperation.targets = getChildTargets(parentOperation);
    }
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

/** Update measurement-result indices for a specific wire. */
const _updateMeasurementLines = (model: CircuitModel, wireIndex: number) => {
  model.ensureQubitCount(wireIndex);
  let resultIndex = 0;
  for (const col of model.componentGrid) {
    for (const comp of col.components) {
      if (comp.kind === "measurement") {
        // Find measurements on the correct wire based on their qubit.
        const qubit = comp.qubits.find((qubit) => qubit.qubit === wireIndex);
        if (qubit) {
          // Remove any existing results and add a new one with the updated index.
          comp.results = [{ qubit: qubit.qubit, result: resultIndex++ }];
        }
      }
    }
  }
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
