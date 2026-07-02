// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// Core state computation for circuit-vis.
// Implements a small statevector simulator that evaluates the circuit model and
// produces an amplitude map. Intentionally avoids DOM/visualization concerns so
// it can run on the main thread or in a Web Worker.
//
// The complex-number, 2x2 matrix, and gate-matrix definitions are shared with
// the Bloch sphere widget via `../../../quantum-math.js`. That module is
// deliberately three.js-free so it can be bundled into this worker without
// pulling in three.js's ~600 KB. Do NOT switch this import to `cplx.js` --
// that file additionally re-exports the quaternion-driven Rotations engine and
// would drag three into the worker.

import type { ComponentGrid, Operation, Qubit } from "../../circuit.js";
import { evaluateAngleExpression } from "../../angleExpression.js";
import {
  Cplx,
  M2x2,
  PauliX,
  PauliY,
  PauliZ,
  Hadamard,
  SGate,
  TGate,
  SXGate,
  rotationX,
  rotationY,
  rotationZ,
} from "../../../quantum-math.js";

// This holds the complex amplitudes of the different basis states.
export type AmpMap = Record<string, { re: number; im: number }>;

export class UnsupportedStateComputeError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "UnsupportedStateComputeError";
  }
}

function parseTheta(op: Operation): number | undefined {
  const arg = op.args?.[0];
  if (!arg) return undefined;
  const v = evaluateAngleExpression(arg);
  return v;
}

function applySingleQubit(
  state: Cplx[],
  target: number,
  mat: M2x2,
  controls: number[] = [],
): void {
  const N = state.length;
  const mask = 1 << target;
  // Hoist the 4 matrix entries to locals so the hot inner loop is pure
  // multiply/add on already-resolved Cplx instances.
  const m00 = mat.a;
  const m01 = mat.b;
  const m10 = mat.c;
  const m11 = mat.d;
  for (let i = 0; i < N; i += 2 * mask) {
    for (let j = 0; j < mask; j++) {
      const i0 = i + j;
      const i1 = i + j + mask;
      const okControls = controls.every((c) => ((i0 >> c) & 1) === 1);
      if (!okControls) continue;
      const a0 = state[i0];
      const a1 = state[i1];
      state[i0] = m00.mul(a0).add(m01.mul(a1));
      state[i1] = m10.mul(a0).add(m11.mul(a1));
    }
  }
}

export function computeAmpMapForCircuit(
  qubits: Qubit[],
  componentGrid: ComponentGrid,
): AmpMap {
  const n = qubits.length;
  if (n === 0) return {};
  const dim = 1 << n;
  const state: Cplx[] = new Array(dim);
  for (let i = 0; i < dim; i++) state[i] = Cplx.zero;
  state[0] = Cplx.one;

  for (const col of componentGrid) {
    for (const op of col.components) {
      switch (op.kind) {
        case "unitary": {
          const targetQubits = op.targets.map((r) => r.qubit);
          const controls = (op.controls ?? []).map((r) => r.qubit);
          const isAdjoint = op.isAdjoint ?? false;
          if (targetQubits.length !== 1) {
            // Unsupported multi-qubit unitary: skip
            continue;
          }
          const t = targetQubits[0];
          let mat: M2x2 | undefined;
          switch (op.gate) {
            case "X":
              mat = PauliX;
              break;
            case "Y":
              mat = PauliY;
              break;
            case "Z":
              mat = PauliZ;
              break;
            case "H":
              mat = Hadamard;
              break;
            case "S":
              mat = SGate;
              break;
            case "T":
              mat = TGate;
              break;
            case "SX":
              mat = SXGate;
              break;
            case "Rx": {
              const th = parseTheta(op);
              if (th !== undefined) mat = rotationX(th);
              break;
            }
            case "Ry": {
              const th = parseTheta(op);
              if (th !== undefined) mat = rotationY(th);
              break;
            }
            case "Rz": {
              const th = parseTheta(op);
              if (th !== undefined) mat = rotationZ(th);
              break;
            }
            default:
              break;
          }
          if (mat) {
            mat = isAdjoint ? mat.adjoint() : mat;
            applySingleQubit(state, t, mat, controls);
          }
          break;
        }
        case "ket": {
          // Reset is non-unitary and generally produces mixed states.
          // The state visualizer currently only supports pure state vectors.
          if (op.gate === "0") {
            throw new UnsupportedStateComputeError(
              "State visualization does not currently support measurement or ResetZ / |0⟩ reset operations.",
            );
          }
          break;
        }
        case "measurement": {
          // Measurement is non-unitary and generally produces mixed states.
          // The state visualizer currently only supports pure state vectors.
          throw new UnsupportedStateComputeError(
            "State visualization does not currently support measurement or ResetZ / |0⟩ reset operations.",
          );
        }
      }
    }
  }

  const ampMap: AmpMap = {};
  const eps = 1e-12;
  for (let i = 0; i < dim; i++) {
    const a = state[i];
    const p = a.re * a.re + a.im * a.im;
    if (p > eps) {
      // Build bitstring label (editor qubit 0 is the most significant/leftmost bit)
      let bits = "";
      for (let q = 0; q < n; q++) {
        bits += (i >> q) & 1 ? "1" : "0";
      }
      ampMap[bits] = { re: a.re, im: a.im };
    }
  }
  return ampMap;
}
