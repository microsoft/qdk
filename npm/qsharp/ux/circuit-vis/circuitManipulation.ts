// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { ComponentGrid, Operation, Unitary } from "./circuit";
import { CircuitEvents } from "./events";
import { Register } from "./register";
import {
  findOperation,
  findParentArray,
  findParentOperation,
  getChildTargets,
  locationStringToIndexes,
} from "./utils";

/**
 * Move an operation in the circuit.
 *
 * @param circuitEvents The CircuitEvents instance to handle circuit-related events.
 * @param sourceLocation The location string of the source operation.
 * @param targetLocation The location string of the target position.
 * @param sourceWire The wire index of the source operation.
 * @param targetWire The wire index to move the operation to.
 * @param movingControl Whether the operation is being moved as a control.
 * @param insertNewColumn Whether to insert a new column when adding the operation.
 * @returns The moved operation or null if the move was unsuccessful.
 */
const moveOperation = (
  circuitEvents: CircuitEvents,
  sourceLocation: string,
  targetLocation: string,
  sourceWire: number,
  targetWire: number,
  movingControl: boolean,
  insertNewColumn: boolean = false,
): Operation | null => {
  const originalOperation = findOperation(
    circuitEvents.componentGrid,
    sourceLocation,
  );

  if (originalOperation == null) return null;

  // Create a deep copy of the source operation
  const newSourceOperation: Operation = JSON.parse(
    JSON.stringify(originalOperation),
  );

  // Update operation's targets and controls
  _moveY(
    circuitEvents,
    newSourceOperation,
    sourceLocation,
    sourceWire,
    targetWire,
    movingControl,
  );

  // Move horizontally
  _moveX(
    circuitEvents,
    newSourceOperation,
    originalOperation,
    targetLocation,
    insertNewColumn,
  );

  const sourceOperationParent = findParentArray(
    circuitEvents.componentGrid,
    sourceLocation,
  );
  if (sourceOperationParent == null) return null;
  _removeOp(circuitEvents, originalOperation, sourceOperationParent);

  return newSourceOperation;
};

/**
 * Move an operation horizontally.
 *
 * @param circuitEvents The CircuitEvents instance to handle circuit-related events.
 * @param sourceOperation The operation to be moved.
 * @param originalOperation The original source operation to be ignored during the check for existing operations.
 * @param targetLocation The location string of the target position.
 * @param insertNewColumn Whether to insert a new column when adding the operation.
 */
const _moveX = (
  circuitEvents: CircuitEvents,
  sourceOperation: Operation,
  originalOperation: Operation,
  targetLocation: string,
  insertNewColumn: boolean = false,
) => {
  const targetOperationParent = findParentArray(
    circuitEvents.componentGrid,
    targetLocation,
  );

  const targetLastIndex = locationStringToIndexes(targetLocation).pop();

  if (targetOperationParent == null || targetLastIndex == null) return;

  // Insert sourceOperation to target last index
  _addOp(
    circuitEvents,
    sourceOperation,
    targetOperationParent,
    targetLastIndex,
    insertNewColumn,
    originalOperation,
  );
};

/**
 * Move an operation vertically by changing its controls and targets.
 *
 * @param circuitEvents The CircuitEvents instance to handle circuit-related events.
 * @param sourceOperation The operation to be moved.
 * @param sourceLocation The location string of the source operation.
 * @param sourceWire The wire index of the source operation.
 * @param targetWire The wire index to move the operation to.
 * @param movingControl Whether the operation is being moved as a control.
 */
const _moveY = (
  circuitEvents: CircuitEvents,
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
    circuitEvents.componentGrid,
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
 * @param circuitEvents The CircuitEvents instance to handle circuit-related events.
 * @param sourceOperation The operation to be added.
 * @param targetLocation The location string of the target position.
 * @param targetWire The wire index to add the operation to.
 * @param insertNewColumn Whether to insert a new column when adding the operation.
 * @returns The added operation or null if the addition was unsuccessful.
 */
const addOperation = (
  circuitEvents: CircuitEvents,
  sourceOperation: Operation,
  targetLocation: string,
  targetWire: number,
  insertNewColumn: boolean = false,
): Operation | null => {
  const targetOperationParent = findParentArray(
    circuitEvents.componentGrid,
    targetLocation,
  );
  const targetLastIndex = locationStringToIndexes(targetLocation).pop();

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

  _addOp(
    circuitEvents,
    newSourceOperation,
    targetOperationParent,
    targetLastIndex,
    insertNewColumn,
  );

  return newSourceOperation;
};

/**
 * Remove an operation from the circuit.
 *
 * @param circuitEvents The CircuitEvents instance to handle circuit-related events.
 * @param sourceLocation The location string of the operation to be removed.
 */
const removeOperation = (
  circuitEvents: CircuitEvents,
  sourceLocation: string,
) => {
  const sourceOperation = findOperation(
    circuitEvents.componentGrid,
    sourceLocation,
  );
  const sourceOperationParent = findParentArray(
    circuitEvents.componentGrid,
    sourceLocation,
  );

  if (sourceOperation == null || sourceOperationParent == null) return null;

  // Delete sourceOperation
  _removeOp(circuitEvents, sourceOperation, sourceOperationParent);
};

/**
 * Find and remove operations in-place based on a predicate function.
 *
 * @param componentGrid The grid of components to search through.
 * @param pred The predicate function to determine which operations to remove.
 */
const findAndRemoveOperations = (
  componentGrid: ComponentGrid,
  pred: (op: Operation) => boolean,
) => {
  const inPlaceFilter = (
    grid: ComponentGrid,
    pred: (op: Operation) => boolean,
  ) => {
    let i = 0;
    while (i < grid.length) {
      let j = 0;
      while (j < grid[i].components.length) {
        if (!pred(grid[i].components[j])) {
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

  const recursivePred = (op: Operation) => {
    if (pred(op)) return true;
    if (op.children) {
      inPlaceFilter(op.children, (child) => !recursivePred(child));
    }
    return false;
  };

  inPlaceFilter(componentGrid, (op) => !recursivePred(op));
};

/**
 * Add a control to the specified operation on the given wire index.
 *
 * @param op The unitary operation to which the control will be added.
 * @param wireIndex The index of the wire where the control will be added.
 * @returns True if the control was added, false if it already existed.
 */
const addControl = (op: Unitary, wireIndex: number): boolean => {
  if (!op.controls) {
    op.controls = [];
  }
  const existingControl = op.controls.find(
    (control) => control.qubit === wireIndex,
  );
  if (!existingControl) {
    op.controls.push({ qubit: wireIndex });
    op.controls.sort((a, b) => a.qubit - b.qubit);
    return true;
  }
  return false;
};

/**
 * Remove a control from the specified operation on the given wire index.
 *
 * @param op The unitary operation from which the control will be removed.
 * @param wireIndex The index of the wire where the control will be removed.
 * @returns True if the control was removed, false if it did not exist.
 */
const removeControl = (op: Unitary, wireIndex: number): boolean => {
  if (op.controls) {
    const controlIndex = op.controls.findIndex(
      (control) => control.qubit === wireIndex,
    );
    if (controlIndex !== -1) {
      op.controls.splice(controlIndex, 1);
      return true;
    }
  }
  return false;
};

/**
 * Add an operation to the circuit at the specified location.
 *
 * @param circuitEvents The CircuitEvents instance to handle circuit-related events.
 * @param sourceOperation The operation to be added.
 * @param targetOperationParent The parent grid where the operation will be added.
 * @param targetLastIndex The index within the parent array where the operation will be added.
 * @param insertNewColumn Whether to insert a new column when adding the operation.
 * @param originalOperation The original source operation to be ignored during the check for existing operations.
 */
const _addOp = (
  circuitEvents: CircuitEvents,
  sourceOperation: Operation,
  targetOperationParent: ComponentGrid,
  targetLastIndex: [number, number],
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
      if (
        (opMinTarget >= minTarget && opMinTarget <= maxTarget) ||
        (opMaxTarget >= minTarget && opMaxTarget <= maxTarget) ||
        (minTarget >= opMinTarget && minTarget <= opMaxTarget) ||
        (maxTarget >= opMinTarget && maxTarget <= opMaxTarget)
      ) {
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

  if (sourceOperation.kind === "measurement") {
    for (const targetWires of sourceOperation.qubits) {
      _updateMeasurementLines(circuitEvents, targetWires.qubit);
    }
  }
};

/**
 * Get the minimum and maximum register indices for a given operation.
 * Based on getMinMaxRegIdx in process.ts, but without the numQubits.
 *
 * @param operation The operation for which to get the register indices.
 * @returns A tuple containing the minimum and maximum register indices.
 */
const _getMinMaxRegIdx = (operation: Operation): [number, number] => {
  let targets: Register[];
  let controls: Register[];
  switch (operation.kind) {
    case "measurement":
      targets = operation.results;
      controls = operation.qubits;
      break;
    case "unitary":
      targets = operation.targets;
      controls = operation.controls || [];
      break;
    case "ket":
      targets = operation.targets;
      controls = [];
      break;
  }

  const ctrls: Register[] = controls || [];
  const qRegs: Register[] = [...ctrls, ...targets].filter(
    ({ result }) => result === undefined,
  );
  if (qRegs.length === 0) return [-1, -1];
  const qRegIdxList: number[] = qRegs.map(({ qubit }) => qubit);
  // Pad the contiguous range of registers that it covers.
  const minRegIdx: number = Math.min(...qRegIdxList);
  const maxRegIdx: number = Math.max(...qRegIdxList);

  return [minRegIdx, maxRegIdx];
};

/**
 * Check if an operation is classically controlled.
 *
 * @param operation The operation for which to get the register indices.
 * @returns True if the operation is classically controlled, false otherwise.
 */
const _isClassicallyControlled = (operation: Operation): boolean => {
  if (operation.kind !== "unitary") return false;
  if (operation.controls === undefined) return false;
  const clsControl = operation.controls.find(
    ({ result }) => result !== undefined,
  );
  return clsControl !== undefined;
};

/**
 * Remove an operation from the circuit.
 *
 * @param circuitEvents The CircuitEvents instance to handle circuit-related events.
 * @param sourceOperation The operation to be removed.
 * @param sourceOperationParent The parent grid from which the operation will be removed.
 */
const _removeOp = (
  circuitEvents: CircuitEvents,
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

  if (sourceOperation.kind === "measurement") {
    for (const result of sourceOperation.results) {
      _updateMeasurementLines(circuitEvents, result.qubit);
    }
  }
};

/**
 * Update measurement lines for a specific wire.
 *
 * @param circuitEvents The CircuitEvents instance to handle circuit-related events.
 * @param wireIndex The index of the wire to update the measurement lines for.
 */
const _updateMeasurementLines = (
  circuitEvents: CircuitEvents,
  wireIndex: number,
) => {
  let resultIndex = 0;
  for (const col of circuitEvents.componentGrid) {
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
  circuitEvents.qubits[wireIndex].numResults =
    resultIndex > 0 ? resultIndex : undefined;
};

export {
  moveOperation,
  addOperation,
  removeOperation,
  findAndRemoveOperations,
  addControl,
  removeControl,
};
