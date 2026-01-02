// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { ComponentGrid, Operation, Qubit } from "./circuit.js";
import { AmpMap } from "./stateViz.js";
import { getCurrentCircuitModel } from "./events.js";

export type Endianness = "big" | "little";

// Small complex helpers
class Complex {
  constructor(
    public re: number,
    public im: number,
  ) {}
  static add(a: Complex, b: Complex) {
    return new Complex(a.re + b.re, a.im + b.im);
  }
  static mul(a: Complex, b: Complex) {
    return new Complex(a.re * b.re - a.im * b.im, a.re * b.im + a.im * b.re);
  }
}

// Matrices for single-qubit gates
const GATE = {
  X: [
    new Complex(0, 0),
    new Complex(1, 0),
    new Complex(1, 0),
    new Complex(0, 0),
  ],
  Y: [
    new Complex(0, 0),
    new Complex(0, -1),
    new Complex(0, 1),
    new Complex(0, 0),
  ], // [[0,-i],[i,0]]
  Z: [
    new Complex(1, 0),
    new Complex(0, 0),
    new Complex(0, 0),
    new Complex(-1, 0),
  ],
  H: [
    new Complex(1 / Math.SQRT2, 0),
    new Complex(1 / Math.SQRT2, 0),
    new Complex(1 / Math.SQRT2, 0),
    new Complex(-1 / Math.SQRT2, 0),
  ],
  S: [
    new Complex(1, 0),
    new Complex(0, 0),
    new Complex(0, 0),
    new Complex(0, 1),
  ], // [[1,0],[0,i]]
  T: [
    new Complex(1, 0),
    new Complex(0, 0),
    new Complex(0, 0),
    new Complex(Math.cos(Math.PI / 4), Math.sin(Math.PI / 4)),
  ],
  SX: [
    // sqrt(X)
    new Complex(0.5 + 0, 0.5),
    new Complex(0.5 - 0, -0.5),
    new Complex(0.5 - 0, -0.5),
    new Complex(0.5 + 0, 0.5),
  ],
};

function rotationX(theta: number) {
  const c = Math.cos(theta / 2);
  const s = Math.sin(theta / 2);
  return [
    new Complex(c, 0),
    new Complex(0, -s),
    new Complex(0, -s),
    new Complex(c, 0),
  ];
}
function rotationY(theta: number) {
  const c = Math.cos(theta / 2);
  const s = Math.sin(theta / 2);
  return [
    new Complex(c, 0),
    new Complex(-s, 0),
    new Complex(s, 0),
    new Complex(c, 0),
  ];
}
function rotationZ(theta: number) {
  const eNeg = new Complex(Math.cos(-theta / 2), Math.sin(-theta / 2));
  const ePos = new Complex(Math.cos(theta / 2), Math.sin(theta / 2));
  return [eNeg, new Complex(0, 0), new Complex(0, 0), ePos];
}

// Evaluate a simple arithmetic expression supporting numbers, + - * /, parentheses, and π
export function evaluateAngleExpression(expr: string): number | undefined {
  if (!expr) return undefined;
  const src = expr.trim().replace(/pi/gi, "π");

  // Tokenizer
  type Tok = { type: "num" | "pi" | "op" | "lpar" | "rpar"; value?: string };
  const toks: Tok[] = [];
  let i = 0;
  while (i < src.length) {
    const ch = src[i];
    if (ch === " " || ch === "\t" || ch === "\n") {
      i++;
      continue;
    }
    if (ch === "(" || ch === ")") {
      toks.push({ type: ch === "(" ? "lpar" : "rpar" });
      i++;
      continue;
    }
    if (ch === "+" || ch === "-" || ch === "*" || ch === "/") {
      toks.push({ type: "op", value: ch });
      i++;
      continue;
    }
    if (ch === "π") {
      toks.push({ type: "pi" });
      i++;
      continue;
    }
    // number: digits with optional decimal point; allow leading dot e.g. .5
    if (ch === "." || /\d/.test(ch)) {
      let j = i + 1;
      while (j < src.length && /[0-9.]/.test(src[j])) j++;
      const numStr = src.slice(i, j);
      // Reject malformed like multiple dots
      if (!/^\d*(?:\.\d+)?$/.test(numStr) && !/^\.\d+$/.test(numStr))
        return undefined;
      toks.push({ type: "num", value: numStr });
      i = j;
      continue;
    }
    // Unknown char
    return undefined;
  }

  // Recursive descent parser
  let k = 0;
  const peek = () => toks[k];
  const consume = () => toks[k++];

  const parseExpr = (): number | undefined => {
    let lhs = parseTerm();
    if (lhs === undefined) return undefined;
    while (
      peek() &&
      peek().type === "op" &&
      (peek().value === "+" || peek().value === "-")
    ) {
      const op = consume().value!;
      const rhs = parseTerm();
      if (rhs === undefined) return undefined;
      lhs = op === "+" ? lhs + rhs : lhs - rhs;
    }
    return lhs;
  };

  const parseTerm = (): number | undefined => {
    let lhs = parseFactor();
    if (lhs === undefined) return undefined;
    while (
      peek() &&
      peek().type === "op" &&
      (peek().value === "*" || peek().value === "/")
    ) {
      const op = consume().value!;
      const rhs = parseFactor();
      if (rhs === undefined) return undefined;
      lhs = op === "*" ? lhs * rhs : lhs / rhs;
    }
    return lhs;
  };

  const parseFactor = (): number | undefined => {
    // unary +/-
    let sign = 1;
    while (
      peek() &&
      peek().type === "op" &&
      (peek().value === "+" || peek().value === "-")
    ) {
      const s = consume().value!;
      sign *= s === "-" ? -1 : 1;
    }
    const t = peek();
    if (!t) return undefined;
    if (t.type === "num") {
      consume();
      const v = t.value === "." ? 0 : parseFloat(t.value!);
      return sign * v;
    }
    if (t.type === "pi") {
      consume();
      return sign * Math.PI;
    }
    if (t.type === "lpar") {
      consume();
      const v = parseExpr();
      if (peek() && peek().type === "rpar") consume();
      else return undefined;
      return sign * (v ?? 0);
    }
    return undefined;
  };

  const result = parseExpr();
  // All tokens must be consumed and result finite
  if (result === undefined || k !== toks.length || !isFinite(result))
    return undefined;
  return result;
}

function parseTheta(op: Operation): number | undefined {
  const arg = op.args?.[0];
  if (!arg) return undefined;
  const v = evaluateAngleExpression(arg);
  return v;
}

function applySingleQubit(
  state: Complex[],
  target: number,
  mat: Complex[],
  controls: number[] = [],
): void {
  const N = state.length;
  const mask = 1 << target;
  for (let i = 0; i < N; i += 2 * mask) {
    for (let j = 0; j < mask; j++) {
      const i0 = i + j;
      const i1 = i + j + mask;
      const okControls = controls.every((c) => ((i0 >> c) & 1) === 1);
      if (!okControls) continue;
      const a0 = state[i0];
      const a1 = state[i1];
      const n0 = Complex.add(Complex.mul(mat[0], a0), Complex.mul(mat[1], a1));
      const n1 = Complex.add(Complex.mul(mat[2], a0), Complex.mul(mat[3], a1));
      state[i0] = n0;
      state[i1] = n1;
    }
  }
}

function applyResetZero(state: Complex[], target: number): void {
  const mask = 1 << target;
  const N = state.length;
  for (let i = 0; i < N; i += 2 * mask) {
    for (let j = 0; j < mask; j++) {
      const i0 = i + j;
      const i1 = i + j + mask;
      const a0 = state[i0];
      const a1 = state[i1];
      const p = a0.re * a0.re + a0.im * a0.im + a1.re * a1.re + a1.im * a1.im;
      const newMag = Math.sqrt(p);

      // Choose a phase direction to preserve: prefer the larger component's phase;
      // if magnitudes tie, prefer the sum's direction if nonzero.
      const mag0 = Math.hypot(a0.re, a0.im);
      const mag1 = Math.hypot(a1.re, a1.im);
      const sumRe = a0.re + a1.re;
      const sumIm = a0.im + a1.im;
      const sumMag = Math.hypot(sumRe, sumIm);

      let dirRe = 1;
      let dirIm = 0;
      if (mag0 === 0 && mag1 === 0) {
        dirRe = 1;
        dirIm = 0;
      } else if (Math.abs(mag0 - mag1) < 1e-12 && sumMag > 1e-15) {
        dirRe = sumRe / sumMag;
        dirIm = sumIm / sumMag;
      } else {
        const t = mag0 >= mag1 ? a0 : a1;
        const tMag = Math.hypot(t.re, t.im);
        if (tMag > 0) {
          dirRe = t.re / tMag;
          dirIm = t.im / tMag;
        }
      }

      state[i0] = new Complex(dirRe * newMag, dirIm * newMag);
      state[i1] = new Complex(0, 0);
    }
  }
}

export function computeAmpMapForCircuit(
  qubits: Qubit[],
  componentGrid: ComponentGrid,
  endianness: Endianness = "big",
): AmpMap {
  const n = qubits.length;
  const dim = 1 << n;
  const state: Complex[] = new Array(dim);
  for (let i = 0; i < dim; i++) state[i] = new Complex(0, 0);
  state[0] = new Complex(1, 0);

  for (const col of componentGrid) {
    for (const op of col.components) {
      switch (op.kind) {
        case "unitary": {
          const targetQubits = op.targets.map((r) => r.qubit);
          const controls = (op.controls ?? []).map((r) => r.qubit);
          if (targetQubits.length !== 1) {
            // Unsupported multi-qubit unitary: skip
            continue;
          }
          const t = targetQubits[0];
          let mat: Complex[] | undefined;
          switch (op.gate) {
            case "X":
              mat = GATE.X;
              break;
            case "Y":
              mat = GATE.Y;
              break;
            case "Z":
              mat = GATE.Z;
              break;
            case "H":
              mat = GATE.H;
              break;
            case "S":
              mat = GATE.S;
              break;
            case "T":
              mat = GATE.T;
              break;
            case "SX":
              mat = GATE.SX;
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
          if (mat) applySingleQubit(state, t, mat, controls);
          break;
        }
        case "ket": {
          // Only support resetting to |0⟩ for now
          if (op.gate === "0" && op.targets.length === 1) {
            applyResetZero(state, op.targets[0].qubit);
          }
          break;
        }
        case "measurement": {
          // Ignore measurement for amplitude computation
          break;
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
      // Build bitstring label per requested endianness
      let bits = "";
      if (endianness === "big") {
        for (let q = n - 1; q >= 0; q--) {
          bits += (i >> q) & 1 ? "1" : "0";
        }
      } else {
        for (let q = 0; q < n; q++) {
          bits += (i >> q) & 1 ? "1" : "0";
        }
      }
      ampMap[bits] = { re: a.re, im: a.im };
    }
  }
  return ampMap;
}

export function computeAmpMapFromCurrentModel(
  endianness: Endianness = "big",
): AmpMap | null {
  const model = getCurrentCircuitModel();
  if (!model) return null;
  return computeAmpMapForCircuit(model.qubits, model.componentGrid, endianness);
}
