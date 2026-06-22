// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/*
 * Pure helpers for validating Bloch-sphere gate-code input.
 *
 * Kept in a separate module from bloch.tsx so they can be unit-tested
 * directly under plain Node without dragging in three.js, preact, or the
 * JSON data tables that bloch.tsx imports.
 */

import {
  PauliX,
  PauliY,
  PauliZ,
  SGate,
  TGate,
  Hadamard,
} from "../quantum-math.js";

/**
 * Axis names accepted by the renderer's animated rotation methods and by
 * `BlochRenderer.snapTo`. Distinct from x/y/z labels in the visualization;
 * these are the rotation primitives the renderer exposes.
 */
export type RotationAxis = "X" | "Y" | "Z" | "H";

/**
 * Per-gate metadata used by both the visualization layer (to animate or
 * snap the sphere) and the math layer (to display the LaTeX equation and
 * update the basis-coefficient state vector). Keyed by the single-character
 * gate code (see `VALID_GATE_CODES`).
 *
 * Keeping one table avoids the previous duplication where the same code was
 * mentioned in a `switch` in the React component, a separate `gateLaTeX`
 * dictionary, and the `cplx` matrix imports.
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
export const VALID_GATE_CODES = "XYZHSsTt";

/**
 * Maximum number of gates accepted from a single untrusted input (URL
 * parameter, paste into the Run textbox, etc.). Each gate animates for
 * ~100ms so even the cap here represents ~25 seconds of replay, which is
 * already past the "this is bad UX" line. The cap exists to bound the
 * abuse case (a stale or hostile link with thousands of gates flooding
 * the animation queue), not to define the intended UX limit.
 */
export const MAX_GATE_SEQUENCE_LENGTH = 256;

const validGateSet = new Set(VALID_GATE_CODES);

/**
 * Filter a string of gate codes down to characters in `VALID_GATE_CODES`
 * and cap its length at `MAX_GATE_SEQUENCE_LENGTH`. Returns the cleaned
 * string and a flag indicating whether anything was dropped, so callers
 * can decide whether to surface a hint to the user.
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
