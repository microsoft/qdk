// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { ComponentGrid, Qubit } from "./circuit.js";

/**
 * Information about a single measurement result wire.
 */
export interface WireInfo {
  /** Whether this result feeds a ClassicalControlled gate. */
  isUsedAsControl: boolean;
  /** Column index where the measurement operation appears. */
  measurementColumn: number;
  /** Column indices of consuming ClassicalControlled gates. */
  controlColumns: number[];
}

/**
 * Per-result key: `${qubit}-${result}`.
 */
type WireKey = string;

function wireKey(qubit: number, result: number): WireKey {
  return `${qubit}-${result}`;
}

/**
 * Full layout information for classical wires in a circuit.
 */
export interface ClassicalWireLayout {
  /**
   * Maps each result wire key to the slot it was assigned.
   * Only results that are used as controls have slot entries.
   */
  slotAssignment: Map<WireKey, number>;
  /** Maximum number of slots needed per qubit. */
  maxSlots: Map<number, number>;
  /**
   * Per-wire horizontal render range (column indices).
   * Only present for results used as controls.
   */
  wireRanges: Map<WireKey, { startCol: number; endCol: number }>;
  /** Full wire info for every measurement result. */
  wireInfos: Map<WireKey, WireInfo>;
}

/**
 * Scan the component grid and determine, for each measurement result,
 * whether it is used as a control for a classically-controlled gate
 * and which columns the measurement and the consuming gates appear in.
 */
export function analyzeClassicalWireUsage(
  componentGrid: ComponentGrid,
): Map<WireKey, WireInfo> {
  const wireInfos = new Map<WireKey, WireInfo>();

  // Pass 1: collect all measurement operations and their results.
  collectMeasurements(componentGrid, wireInfos);

  // Pass 2: find classically-controlled gates that consume results.
  collectControls(componentGrid, wireInfos);

  return wireInfos;
}

function collectMeasurements(
  grid: ComponentGrid,
  wireInfos: Map<WireKey, WireInfo>,
  baseCol = 0,
): void {
  grid.forEach((col, colIndex) => {
    const absCol = baseCol + colIndex;
    for (const op of col.components) {
      if (op.kind === "measurement" && op.results) {
        for (const reg of op.results) {
          if (reg.result != null) {
            const key = wireKey(reg.qubit, reg.result);
            if (!wireInfos.has(key)) {
              wireInfos.set(key, {
                isUsedAsControl: false,
                measurementColumn: absCol,
                controlColumns: [],
              });
            }
          }
        }
      }
      if (op.children) {
        collectMeasurements(op.children, wireInfos, absCol);
      }
    }
  });
}

function collectControls(
  grid: ComponentGrid,
  wireInfos: Map<WireKey, WireInfo>,
  baseCol = 0,
): void {
  grid.forEach((col, colIndex) => {
    const absCol = baseCol + colIndex;
    for (const op of col.components) {
      if (op.isConditional && op.kind === "unitary" && op.controls) {
        for (const ctrl of op.controls) {
          if (ctrl.result != null) {
            const key = wireKey(ctrl.qubit, ctrl.result);
            const info = wireInfos.get(key);
            if (info) {
              info.isUsedAsControl = true;
              info.controlColumns.push(absCol);
            }
          }
        }
      }
      if (op.children) {
        collectControls(op.children, wireInfos, absCol);
      }
    }
  });
}

/**
 * Greedy interval slot assignment for a single qubit's used results.
 *
 * Results NOT used as controls → no slot (stub only).
 * Results used as controls → assigned to the first slot whose
 * `[measurementCol, maxControlCol]` range doesn't overlap with
 * existing assignments.
 *
 * @returns Map from result index to slot index, plus the total number of slots used.
 */
export function assignClassicalWireSlots(
  usedWires: { resultIndex: number; startCol: number; endCol: number }[],
): { assignment: Map<number, number>; maxSlots: number } {
  const assignment = new Map<number, number>();
  // Each slot tracks the rightmost endCol currently occupying it.
  const slotEnds: number[] = [];

  // Sort by start column for greedy assignment.
  const sorted = [...usedWires].sort((a, b) => a.startCol - b.startCol);

  for (const wire of sorted) {
    let assigned = false;
    for (let s = 0; s < slotEnds.length; s++) {
      if (slotEnds[s] < wire.startCol) {
        // Non-overlapping — reuse this slot.
        slotEnds[s] = wire.endCol;
        assignment.set(wire.resultIndex, s);
        assigned = true;
        break;
      }
    }
    if (!assigned) {
      // Allocate a new slot.
      assignment.set(wire.resultIndex, slotEnds.length);
      slotEnds.push(wire.endCol);
    }
  }

  return { assignment, maxSlots: slotEnds.length };
}

/**
 * Orchestrating function that produces the full ClassicalWireLayout
 * for a circuit by analyzing usage and assigning slots.
 */
export function computeClassicalWireLayout(
  componentGrid: ComponentGrid,
  qubits: Qubit[],
): ClassicalWireLayout {
  const wireInfos = analyzeClassicalWireUsage(componentGrid);
  const slotAssignment = new Map<WireKey, number>();
  const maxSlots = new Map<number, number>();
  const wireRanges = new Map<WireKey, { startCol: number; endCol: number }>();

  // Group wire infos by qubit.
  const perQubit = new Map<number, { key: WireKey; info: WireInfo }[]>();
  for (const [key, info] of wireInfos) {
    const qubitId = parseInt(key.split("-")[0], 10);
    if (!perQubit.has(qubitId)) perQubit.set(qubitId, []);
    perQubit.get(qubitId)!.push({ key, info });
  }

  for (const qubit of qubits) {
    const entries = perQubit.get(qubit.id) || [];
    const usedWires: {
      resultIndex: number;
      startCol: number;
      endCol: number;
      key: WireKey;
    }[] = [];

    for (const { key, info } of entries) {
      if (info.isUsedAsControl) {
        const resultIndex = parseInt(key.split("-")[1], 10);
        const endCol = Math.max(...info.controlColumns);
        usedWires.push({
          resultIndex,
          startCol: info.measurementColumn,
          endCol,
          key,
        });
        wireRanges.set(key, {
          startCol: info.measurementColumn,
          endCol,
        });
      }
    }

    const { assignment, maxSlots: slotCount } =
      assignClassicalWireSlots(usedWires);
    maxSlots.set(qubit.id, slotCount);

    for (const wire of usedWires) {
      const slot = assignment.get(wire.resultIndex);
      if (slot != null) {
        slotAssignment.set(wire.key, slot);
      }
    }
  }

  return { slotAssignment, maxSlots, wireRanges, wireInfos };
}
