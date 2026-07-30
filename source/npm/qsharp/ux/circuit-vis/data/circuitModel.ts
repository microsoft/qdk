// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { getOperationRegisters } from "../utils.js";
import { Circuit, ComponentGrid, Operation, Qubit } from "./circuit.js";

/**
 * `CircuitModel` — the persistent circuit definition (the Data layer
 * of the Data / Action / View architecture).
 *
 * Owns:
 *   - `componentGrid` — the grid of operations.
 *   - `qubits`        — the qubit lines (wires).
 *   - `qubitUseCounts`— per-wire op-use counts (derived state,
 *                       maintained incrementally).
 *
 * Maintains its own invariants (qubit count, use counts) but does not
 * perform user-level edits — those live in
 * [circuitActions.ts](circuitActions.ts), which take a `CircuitModel`
 * and mutate it in place. No DOM, SVG, rendering, or interaction
 * state, which keeps `circuitActions.*` unit-testable without JSDOM.
 */
export class CircuitModel {
  /** The grid of components rendered as columns of operations. */
  componentGrid: ComponentGrid;

  /** The qubits/wires in this circuit. */
  qubits: Qubit[];

  /**
   * Per-wire op-use counts. `qubitUseCounts[i]` is the number of
   * operations whose register list includes qubit `i`. Used by
   * `removeTrailingUnusedQubits` to drop unused trailing wires.
   * Maintained incrementally by the `increment...` / `decrement...`
   * methods, which Actions call when adding/removing an op.
   */
  qubitUseCounts: number[];

  /**
   * Build a `CircuitModel` from an existing `Circuit`. `componentGrid`
   * and `qubits` are borrowed by reference, not copied, so the
   * renderer's `Sqore` and the editor's `CircuitEvents` share the same
   * data.
   */
  constructor(circuit: Circuit) {
    this.componentGrid = circuit.componentGrid;
    this.qubits = circuit.qubits;
    this.qubitUseCounts = new Array(this.qubits.length).fill(0);
    for (const column of this.componentGrid) {
      for (const op of column.components) {
        this.incrementQubitUseCountForOp(op);
      }
    }
  }

  /**
   * Return the underlying `Circuit` shape for read-only consumers.
   * The result aliases the model's arrays — callers needing a deep
   * copy must clone explicitly.
   */
  snapshot(): Circuit {
    return { qubits: this.qubits, componentGrid: this.componentGrid };
  }

  /**
   * Grow `qubits` (and `qubitUseCounts`) so that `wireIndex` is a
   * valid wire index. No-op if the model already has at least
   * `wireIndex + 1` wires.
   */
  ensureQubitCount(wireIndex: number): void {
    while (this.qubits.length <= wireIndex) {
      this.qubits.push({ id: this.qubits.length, numResults: undefined });
      this.qubitUseCounts.push(0);
    }
  }

  /**
   * Drop trailing wires that no operation references anywhere in the
   * tree (including a group op's derived `.targets` / `.results`).
   *
   * Walks the grid directly rather than consulting `qubitUseCounts`:
   * a group op's derived `.targets` can be rewritten without a
   * matching count adjustment, so the counter can report a wire as
   * unused while the group still names it — dropping such a wire
   * would crash the next render.
   */
  removeTrailingUnusedQubits(): void {
    let maxUsed = -1;
    const walk = (grid: ComponentGrid): void => {
      for (const col of grid) {
        for (const op of col.components) {
          for (const reg of getOperationRegisters(op)) {
            if (reg.result === undefined && reg.qubit > maxUsed) {
              maxUsed = reg.qubit;
            }
          }
          if (op.children) walk(op.children);
        }
      }
    };
    walk(this.componentGrid);

    while (this.qubits.length > maxUsed + 1) {
      this.qubits.pop();
      this.qubitUseCounts.pop();
    }
  }

  /**
   * Bump `qubitUseCounts[i]` for every qubit register `i` referenced
   * by `op` (skips classical-result registers). Out-of-range wires
   * are silently ignored.
   */
  incrementQubitUseCountForOp(op: Operation): void {
    for (const reg of getOperationRegisters(op)) {
      if (
        reg.result === undefined &&
        reg.qubit >= 0 &&
        reg.qubit < this.qubitUseCounts.length
      ) {
        this.qubitUseCounts[reg.qubit]++;
      }
    }
  }

  /** Mirror of `incrementQubitUseCountForOp`; called when removing an op. */
  decrementQubitUseCountForOp(op: Operation): void {
    for (const reg of getOperationRegisters(op)) {
      if (
        reg.result === undefined &&
        reg.qubit >= 0 &&
        reg.qubit < this.qubitUseCounts.length
      ) {
        this.qubitUseCounts[reg.qubit]--;
      }
    }
  }
}
