// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/*
 * Pure gate-code metadata and validation for the Bloch-sphere widget.
 *
 * Kept separate from bloch.tsx so it can be unit-tested under plain Node
 * without pulling in three.js, preact, or the JSON data tables.
 */

import {
  PauliX,
  PauliY,
  PauliZ,
  SGate,
  TGate,
  Hadamard,
} from "../quantum-math.js";

/** Rotation primitives the renderer exposes (distinct from the x/y/z axis
 * labels in the visualization). */
export type RotationAxis = "X" | "Y" | "Z" | "H";

/**
 * Per-gate metadata, keyed by single-character gate code, shared by the
 * visualization layer (to animate/snap) and the math layer (to display
 * the LaTeX equation and update the state vector).
 */
export const gateInfo: Record<
  string,
  {
    /** Display name for the LaTeX equation header (e.g. "X", "S\u2020"). */
    display: string;
    /** The 2x2 matrix in the computational basis. */
    matrix: typeof PauliX;
    /** Pre-rendered LaTeX for the matrix used in the trace pane. */
    latex: string;
    /** Which renderer rotation primitive to invoke. */
    rotateAxis: RotationAxis;
    /** Angle in radians (sign matters for adjoint variants). */
    rotateAngle: number;
  }
> = {
  X: {
    display: "X",
    matrix: PauliX,
    latex: "\\begin{bmatrix} 0 & 1 \\\\ 1 & 0 \\end{bmatrix}",
    rotateAxis: "X",
    rotateAngle: Math.PI,
  },
  Y: {
    display: "Y",
    matrix: PauliY,
    latex: "\\begin{bmatrix} 0 & -i \\\\ i & 0 \\end{bmatrix}",
    rotateAxis: "Y",
    rotateAngle: Math.PI,
  },
  Z: {
    display: "Z",
    matrix: PauliZ,
    latex: "\\begin{bmatrix} 1 & 0 \\\\ 0 & -1 \\end{bmatrix}",
    rotateAxis: "Z",
    rotateAngle: Math.PI,
  },
  S: {
    display: "S",
    matrix: SGate,
    latex:
      "\\begin{bmatrix} 1 & 0 \\\\ 0 & e^{i {\\pi \\over 2}} \\end{bmatrix}",
    rotateAxis: "Z",
    rotateAngle: Math.PI / 2,
  },
  s: {
    display: "S\u2020",
    matrix: SGate.adjoint(),
    latex:
      "\\begin{bmatrix} 1 & 0 \\\\ 0 & e^{-i {\\pi \\over 2}} \\end{bmatrix}",
    rotateAxis: "Z",
    rotateAngle: -Math.PI / 2,
  },
  T: {
    display: "T",
    matrix: TGate,
    latex:
      "\\begin{bmatrix} 1 & 0 \\\\ 0 & e^{i {\\pi \\over 4}} \\end{bmatrix}",
    rotateAxis: "Z",
    rotateAngle: Math.PI / 4,
  },
  t: {
    display: "T\u2020",
    matrix: TGate.adjoint(),
    latex:
      "\\begin{bmatrix} 1 & 0 \\\\ 0 & e^{-i {\\pi \\over 4}} \\end{bmatrix}",
    rotateAxis: "Z",
    rotateAngle: -Math.PI / 4,
  },
  H: {
    display: "H",
    matrix: Hadamard,
    latex:
      "{1 \\over \\sqrt{2}} \\begin{bmatrix} 1 & 1 \\\\ 1 & -1 \\end{bmatrix}",
    rotateAxis: "H",
    rotateAngle: Math.PI,
  },
};

/**
 * The set of single-character gate codes the widget understands. Each
 * character here must correspond to a `case` arm in `BlochSphere.rotate`.
 */
/** The single-character gate codes the widget understands. */
export const VALID_GATE_CODES = "XYZHSsTt";

/**
 * Cap on gates accepted from a single untrusted input (URL parameter,
 * paste, etc.). Bounds the abuse case (a hostile link with thousands of
 * gates flooding the animation queue), not the intended UX limit.
 */
export const MAX_GATE_SEQUENCE_LENGTH = 256;

const validGateSet = new Set(VALID_GATE_CODES);

/**
 * Filter a string down to `VALID_GATE_CODES` and cap its length. Returns
 * the cleaned string and whether anything was dropped.
 */
export function sanitizeGateSequence(input: string | undefined | null): {
  gates: string;
  modified: boolean;
} {
  if (!input) return { gates: "", modified: false };
  let filtered = "";
  for (const ch of input) {
    if (validGateSet.has(ch)) filtered += ch;
  }
  const capped = filtered.slice(0, MAX_GATE_SEQUENCE_LENGTH);
  return {
    gates: capped,
    modified: capped.length !== input.length,
  };
}
