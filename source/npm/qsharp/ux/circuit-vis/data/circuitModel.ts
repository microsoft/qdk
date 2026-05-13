// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { getOperationRegisters } from "../utils.js";
import { Circuit, ComponentGrid, Operation, Qubit } from "./circuit.js";

/**
 * `CircuitModel` — the persistent circuit definition.
 *
 * This is the **Data layer** in the circuit editor's three-layer
 * architecture (Data / Action / View — see [CIRCUIT_EDITOR_TODO.md](CIRCUIT_EDITOR_TODO.md)).
 * Owns three pieces of state:
 *
 *   - `componentGrid` — the grid of operations the user sees and edits.
 *   - `qubits`        — the list of qubit lines (wires).
 *   - `qubitUseCounts`— how many ops touch each wire; derived state, but
 *                       maintained incrementally because recomputing on
 *                       every edit would be wasteful.
 *
 * The model knows how to **maintain its own invariants** (qubit count,
 * use counts). It does **not** know how to perform user-level edits
 * — those live in [circuitActions.ts](circuitActions.ts), which take a
 * `CircuitModel` as their first argument and mutate it in place.
 *
 * Specifically: no DOM, no SVG, no rendering, no interaction state.
 * That separation is what makes `circuitActions.*` directly unit-testable
 * without JSDOM.
 */
export class CircuitModel {
  /** The grid of components rendered as columns of operations. */
  componentGrid: ComponentGrid;

  /** The qubits/wires in this circuit. */
  qubits: Qubit[];

  /**
   * Per-wire op-use counts. `qubitUseCounts[i]` is the number of
   * operations whose target/control register list includes qubit `i`.
   * Used by `removeTrailingUnusedQubits` to drop wires that no longer
   * carry any operation.
   *
   * Maintained incrementally by `increment...` / `decrement...` —
   * Actions call those whenever they add/remove an op.
   */
  qubitUseCounts: number[];

  /**
   * Build a `CircuitModel` from an existing `Circuit`. The
   * `componentGrid` and `qubits` arrays are **borrowed by reference**,
   * not copied — mutating the model mutates the original `Circuit`.
   * That's intentional: the renderer's `Sqore` and the editor's
   * `CircuitEvents` should see the same data.
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
   * Return the underlying `Circuit` shape for read-only consumers
   * (e.g. the state-viz bridge). Returned object aliases the model's
   * arrays — callers that need a deep copy must clone explicitly.
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
   * Drop trailing wires that no operation references. Stops at the
   * first wire with a non-zero use count from the right.
   */
  removeTrailingUnusedQubits(): void {
    while (
      this.qubitUseCounts.length > 0 &&
      this.qubitUseCounts[this.qubitUseCounts.length - 1] === 0
    ) {
      this.qubits.pop();
      this.qubitUseCounts.pop();
    }
  }

  /**
   * Bump `qubitUseCounts[i]` for every qubit register `i` referenced
   * by `op` (skips classical-result registers). Bounds-checks to
   * tolerate ops that reference wires not yet in the model — those
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
