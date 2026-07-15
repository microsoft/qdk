// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { Column, ComponentGrid, Operation } from "../../data/circuit.js";
import { CircuitModel } from "../../data/circuitModel.js";
import { getMinMaxRegIdx, getOperationRegisters } from "../../utils.js";

/*
 * `gridPrimitives.ts` — low-level component-grid operations shared across the Action layer.
 *
 * Structural primitives every higher-level action builds on: inserting/removing an op into a
 * column, detecting and resolving sibling-column overlaps, measuring an op's drawn span, and
 * renumbering per-wire measurement results. Depend only on the Data layer and `utils.ts`, so they
 * sit at the bottom of the import DAG.
 */

/** Determines whether two register index ranges overlap. */
const doesOverlap = (op1: [number, number], op2: [number, number]): boolean => {
  const [min1, max1] = op1;
  const [min2, max2] = op2;
  return max1 >= min2 && max2 >= min1;
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

/**
 * Update measurement-result indices for a specific wire.
 *
 * Walks the entire grid tree (including nested children) and renumbers every measurement on
 * `wireIndex` in document order, then sets `model.qubits[wireIndex].numResults` to the total.
 * Recursing into children is essential: the renderer reads any measurement's results, including
 * ones inside expanded groups, and throws on an uncounted nested measurement.
 */
const updateMeasurementLines = (model: CircuitModel, wireIndex: number) => {
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

/**
 * Add an operation to the circuit at the specified location.
 */
const addOp = (
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

  // Check if there are any existing operations in the target column within the wire range of the
  // new operation
  if (!insertNewColumn) {
    const [minTarget, maxTarget] = getMinMaxRegIdx(sourceOperation);
    for (const op of targetOperationParent[colIndex].components) {
      if (op === originalOperation) continue;

      const [opMinTarget, opMaxTarget] = getMinMaxRegIdx(op);
      if (doesOverlap([minTarget, maxTarget], [opMinTarget, opMaxTarget])) {
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
      updateMeasurementLines(model, targetWire.qubit);
    }
  }
};

/** Remove an operation from the circuit. */
const removeOp = (
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
      updateMeasurementLines(model, result.qubit);
    }
  }
};

/** Move an element of `arr` from index `from` to index `to`. */
const moveArrayElement = <T>(arr: T[], from: number, to: number) => {
  const el = arr.splice(from, 1)[0];
  arr.splice(to, 0, el);
};

/**
 * Walk `op` and every descendant to find the lowest and highest quantum wire (registers with
 * `result` undefined; classical-ref entries are skipped, as they reference a producer's wire). Used
 * by `moveOperation` to refuse a unit-shift that would push a wire below 0 and to know how far to
 * grow the model. Walking the subtree matters for groups whose root `.targets` may miss deeply
 * nested wires. Returns `[-1, -1]` if the subtree references no quantum wires.
 */
const getSubtreeMinMaxWire = (op: Operation): [number, number] => {
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
 * Resolves overlapping operations in each column of the component grid. For each column, splits
 * overlapping operations into separate columns so that no two operations in the same column overlap
 * on their register ranges. Modifies the component grid in-place.
 */
const resolveOverlappingOperations = (parentArray: ComponentGrid): void => {
  // Helper to resolve a single column into non-overlapping columns
  const resolveColumn = (col: Column): Column[] => {
    const newColumn: Column = { components: [] };
    let [lastMin, lastMax] = [-1, -1];
    let i = 0;
    while (i < col.components.length) {
      const op = col.components[i];
      const [currMin, currMax] = getMinMaxRegIdx(op);
      // Sets up the first operation for comparison or if the current operation doesn't overlap
      if (i === 0 || !doesOverlap([lastMin, lastMax], [currMin, currMax])) {
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

/**
 * Recursive variant of `resolveOverlappingOperations` — resolves overlaps in every column at every
 * nesting level of the grid. Used by `moveQubit`, which can widen group spans anywhere in the tree.
 */
const resolveOverlappingOperationsRecursive = (grid: ComponentGrid): void => {
  resolveOverlappingOperations(grid);
  for (const col of grid) {
    for (const op of col.components) {
      if (op.children != null) {
        resolveOverlappingOperationsRecursive(op.children);
      }
    }
  }
};

export {
  addOp,
  doesOverlap,
  getSubtreeMinMaxWire,
  moveArrayElement,
  removeOp,
  updateMeasurementLines,
  resolveOverlappingOperations,
  resolveOverlappingOperationsRecursive,
};
