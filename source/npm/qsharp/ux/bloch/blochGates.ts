// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/*
 * Pure gate-code metadata and validation for the Bloch-sphere widget.
 *
 * Kept separate from bloch.tsx so it can be unit-tested under plain Node
 * without pulling in three.js, preact, or the JSON data tables.
 */

import {
  M2x2,
  Cplx,
  PauliX,
  PauliY,
  PauliZ,
  SGate,
  TGate,
  Hadamard,
  SXGate,
  rotationX,
  rotationY,
  rotationZ,
} from "../quantum-math.js";

/** Rotation primitives the renderer exposes (distinct from the x/y/z axis
 * labels in the visualization). */
export type RotationAxis = "X" | "Y" | "Z" | "H";

/** Fixed (angle-less) gate kinds. Adjoint variants use a trailing
 * apostrophe -- the same token they serialize to in the readable text form.
 * (The display forms use a dagger glyph; see each entry's `label`/`latexName`.) */
export type FixedGateKind =
  | "X"
  | "Y"
  | "Z"
  | "H"
  | "S"
  | "S'"
  | "T"
  | "T'"
  | "SX"
  | "SX'";
/** Parameterized single-qubit rotation gates (angle in radians). */
export type RotationGateKind = "Rx" | "Ry" | "Rz";

export type GateKind = FixedGateKind | RotationGateKind;

/**
 * A single applied gate. Rotation kinds (`Rx`/`Ry`/`Rz`) carry an `angle`
 * in radians; fixed kinds leave it undefined. This structured token is the
 * canonical unit of the widget's state -- both the readable text form and
 * the compact URL form are serializations of a `Gate[]`.
 */
export interface Gate {
  kind: GateKind;
  /** Rotation angle in radians. Present iff `kind` is a rotation gate. */
  angle?: number;
}

/** Fully-resolved render/math metadata for one gate. */
export interface ResolvedGate {
  /** Plain-text label for HTML (buttons, chips), e.g. "S\u2020", "Rx(1.5708)". */
  label: string;
  /** LaTeX name for the trace equation header, e.g. "S^\\dagger", "R_x(1.5708)". */
  latexName: string;
  /** The 2x2 matrix in the computational basis. */
  matrix: M2x2;
  /** LaTeX for the matrix, used in the trace pane. */
  matrixLatex: string;
  /** Which renderer rotation primitive to invoke. */
  rotateAxis: RotationAxis;
  /** Angle in radians (sign matters for adjoint variants). */
  rotateAngle: number;
}

/**
 * Cap on gates accepted from a single untrusted input (URL parameter,
 * paste, etc.). Bounds the abuse case (a hostile link with thousands of
 * gates flooding the animation queue), not the intended UX limit.
 */
export const MAX_GATE_SEQUENCE_LENGTH = 256;

// Per-fixed-gate metadata. Beyond the render/math fields (which `ResolvedGate`
// already describes), each entry carries its single-char compact-URL code
// (`url`, e.g. `s`, `v`). The kind string itself doubles as the canonical
// readable token (`S'`, `SX'`), and its `base` symbol / adjoint-ness are
// derived from the trailing apostrophe.
interface FixedGateMeta extends ResolvedGate {
  url: string;
}

// Curated LaTeX for the fixed gates' matrices (nicer than a generic
// per-entry render for the ones with exact closed forms). Declaration order
// here defines the palette order (see `FIXED_GATE_KINDS`).
const FIXED_GATE_TABLE: Record<FixedGateKind, FixedGateMeta> = {
  X: {
    label: "X",
    latexName: "X",
    matrix: PauliX,
    matrixLatex: "\\begin{bmatrix} 0 & 1 \\\\ 1 & 0 \\end{bmatrix}",
    rotateAxis: "X",
    rotateAngle: Math.PI,
    url: "X",
  },
  Y: {
    label: "Y",
    latexName: "Y",
    matrix: PauliY,
    matrixLatex: "\\begin{bmatrix} 0 & -i \\\\ i & 0 \\end{bmatrix}",
    rotateAxis: "Y",
    rotateAngle: Math.PI,
    url: "Y",
  },
  Z: {
    label: "Z",
    latexName: "Z",
    matrix: PauliZ,
    matrixLatex: "\\begin{bmatrix} 1 & 0 \\\\ 0 & -1 \\end{bmatrix}",
    rotateAxis: "Z",
    rotateAngle: Math.PI,
    url: "Z",
  },
  H: {
    label: "H",
    latexName: "H",
    matrix: Hadamard,
    matrixLatex:
      "{1 \\over \\sqrt{2}} \\begin{bmatrix} 1 & 1 \\\\ 1 & -1 \\end{bmatrix}",
    rotateAxis: "H",
    rotateAngle: Math.PI,
    url: "H",
  },
  S: {
    label: "S",
    latexName: "S",
    matrix: SGate,
    matrixLatex:
      "\\begin{bmatrix} 1 & 0 \\\\ 0 & e^{i {\\pi \\over 2}} \\end{bmatrix}",
    rotateAxis: "Z",
    rotateAngle: Math.PI / 2,
    url: "S",
  },
  "S'": {
    label: "S\u2020",
    latexName: "S^\\dagger",
    matrix: SGate.adjoint(),
    matrixLatex:
      "\\begin{bmatrix} 1 & 0 \\\\ 0 & e^{-i {\\pi \\over 2}} \\end{bmatrix}",
    rotateAxis: "Z",
    rotateAngle: -Math.PI / 2,
    url: "s",
  },
  T: {
    label: "T",
    latexName: "T",
    matrix: TGate,
    matrixLatex:
      "\\begin{bmatrix} 1 & 0 \\\\ 0 & e^{i {\\pi \\over 4}} \\end{bmatrix}",
    rotateAxis: "Z",
    rotateAngle: Math.PI / 4,
    url: "T",
  },
  "T'": {
    label: "T\u2020",
    latexName: "T^\\dagger",
    matrix: TGate.adjoint(),
    matrixLatex:
      "\\begin{bmatrix} 1 & 0 \\\\ 0 & e^{-i {\\pi \\over 4}} \\end{bmatrix}",
    rotateAxis: "Z",
    rotateAngle: -Math.PI / 4,
    url: "t",
  },
  SX: {
    label: "SX",
    latexName: "SX",
    matrix: SXGate,
    matrixLatex:
      "{1 \\over 2} \\begin{bmatrix} 1+i & 1-i \\\\ 1-i & 1+i \\end{bmatrix}",
    rotateAxis: "X",
    rotateAngle: Math.PI / 2,
    url: "V",
  },
  "SX'": {
    label: "SX\u2020",
    latexName: "SX^\\dagger",
    matrix: SXGate.adjoint(),
    matrixLatex:
      "{1 \\over 2} \\begin{bmatrix} 1-i & 1+i \\\\ 1+i & 1-i \\end{bmatrix}",
    rotateAxis: "X",
    rotateAngle: -Math.PI / 2,
    url: "v",
  },
};

// Maps a rotation kind to its renderer axis and matrix constructor.
const ROTATION_TABLE: Record<
  RotationGateKind,
  { axis: RotationAxis; build: (theta: number) => M2x2; sub: string }
> = {
  Rx: { axis: "X", build: rotationX, sub: "x" },
  Ry: { axis: "Y", build: rotationY, sub: "y" },
  Rz: { axis: "Z", build: rotationZ, sub: "z" },
};

/**
 * Fixed-gate palette order, used both for the clickable palette and the
 * gate-count breakdown chips. Derived from `FIXED_GATE_TABLE`'s declaration
 * order so the gate list lives in exactly one place.
 */
export const FIXED_GATE_KINDS = Object.keys(
  FIXED_GATE_TABLE,
) as FixedGateKind[];

// Lookups derived from `FIXED_GATE_TABLE` so parsing/serialization never
// restate the gate set. A kind's `base` symbol and adjoint-ness come from
// its trailing apostrophe (`S'` -> base `S`, adjoint):
//   - FIXED_BASES / ADJOINTABLE: base symbol -> kind (base vs. apostrophe form)
//   - FIXED_TO_URL / URL_TO_FIXED: kind <-> compact URL char
const FIXED_BASES: Record<string, FixedGateKind> = {};
const ADJOINTABLE: Record<string, FixedGateKind> = {};
const FIXED_TO_URL = {} as Record<FixedGateKind, string>;
const URL_TO_FIXED: Record<string, FixedGateKind> = {};
for (const kind of FIXED_GATE_KINDS) {
  const isAdjoint = kind.endsWith("'");
  const base = isAdjoint ? kind.slice(0, -1) : kind;
  (isAdjoint ? ADJOINTABLE : FIXED_BASES)[base] = kind;
  FIXED_TO_URL[kind] = FIXED_GATE_TABLE[kind].url;
  URL_TO_FIXED[FIXED_GATE_TABLE[kind].url] = kind;
}

/**
 * Serialize an angle for the canonical text and URL forms. Uses the
 * shortest round-trippable decimal JavaScript produces, so a typed value
 * survives intact and dial-produced angles stay short.
 */
export function angleToString(angle: number): string {
  return String(angle);
}

/** Concise angle for display labels (trace headers, chips), capped at four
 * decimals with trailing zeros trimmed. */
function angleForDisplay(angle: number): string {
  return String(Number(angle.toFixed(4)));
}

/** Render a 2x2 matrix as a LaTeX bmatrix, one `Cplx.toLaTeX` per entry. */
function matrixToLaTeX(m: M2x2): string {
  return (
    `\\begin{bmatrix} ${m.a.toLaTeX()} & ${m.b.toLaTeX()} \\\\ ` +
    `${m.c.toLaTeX()} & ${m.d.toLaTeX()} \\end{bmatrix}`
  );
}

/** Resolve a gate token into its render/math metadata. */
export function resolveGate(gate: Gate): ResolvedGate {
  if (gate.kind === "Rx" || gate.kind === "Ry" || gate.kind === "Rz") {
    const angle = gate.angle ?? 0;
    const { axis, build } = ROTATION_TABLE[gate.kind];
    const matrix = build(angle);
    const disp = angleForDisplay(angle);
    const sub = gate.kind[1]; // x / y / z
    return {
      label: `${gate.kind}(${disp})`,
      latexName: `R_${sub}(${disp})`,
      matrix,
      matrixLatex: matrixToLaTeX(matrix),
      rotateAxis: axis,
      rotateAngle: angle,
    };
  }
  return FIXED_GATE_TABLE[gate.kind];
}

// --- Canonical (readable) text form ------------------------------------

// One token: a fixed gate (with optional apostrophe adjoint) OR a rotation
// with a parenthesized decimal-radian angle. Case-insensitive. The
// fixed-base alternation is derived from the gate kinds (apostrophe
// stripped), sorted longest-first so a multi-char base (SX) wins over its
// prefix (S).
const NUMBER_SRC = "-?(?:\\d+\\.?\\d*|\\.\\d+)(?:[eE][+-]?\\d+)?";
const FIXED_BASE_ALTERNATION = [
  ...new Set(FIXED_GATE_KINDS.map((k) => k.replace(/'$/, ""))),
]
  .sort((a, b) => b.length - a.length)
  .join("|");
const FIXED_TOKEN_RE = new RegExp(`^(${FIXED_BASE_ALTERNATION})('?)$`, "i");
const ROTATION_TOKEN_RE = new RegExp(`^R([XYZ])\\((${NUMBER_SRC})\\)$`, "i");

/**
 * Parse the canonical readable text form into gate tokens. Tokens are
 * whitespace-separated. Rotation names are case-insensitive (`Rx`, `rx`, `RX`);
 * adjoint is a trailing apostrophe on S/T/SX. Any token that doesn't
 * fully parse is dropped (so an in-progress token like `Rx(1.5` is
 * simply skipped), and the sequence is capped at `MAX_GATE_SEQUENCE_LENGTH`.
 * `modified` reports whether anything was dropped or capped.
 */
export function parseGateSequence(input: string | undefined | null): {
  gates: Gate[];
  modified: boolean;
} {
  if (!input || !input.trim()) return { gates: [], modified: false };
  const tokens = input.trim().split(/\s+/);
  const gates: Gate[] = [];
  let dropped = false;
  for (const token of tokens) {
    if (!token) continue;
    const rot = token.match(ROTATION_TOKEN_RE);
    if (rot) {
      const axis = rot[1].toUpperCase(); // X / Y / Z
      const angle = Number.parseFloat(rot[2]);
      if (Number.isFinite(angle)) {
        gates.push({
          kind: ("R" + axis.toLowerCase()) as RotationGateKind,
          angle,
        });
        continue;
      }
      dropped = true;
      continue;
    }
    const fixed = token.match(FIXED_TOKEN_RE);
    if (fixed) {
      const base = fixed[1].toUpperCase();
      const isAdjoint = fixed[2] === "'";
      if (isAdjoint) {
        const kind = ADJOINTABLE[base];
        if (kind) {
          gates.push({ kind });
          continue;
        }
        dropped = true; // adjoint not allowed on this gate (e.g. X')
        continue;
      }
      gates.push({ kind: FIXED_BASES[base] });
      continue;
    }
    dropped = true; // unrecognized / incomplete token
  }
  const capped = gates.slice(0, MAX_GATE_SEQUENCE_LENGTH);
  return { gates: capped, modified: dropped || capped.length !== gates.length };
}

/** Serialize gate tokens to the canonical readable text form. */
export function formatGateSequence(gates: Gate[]): string {
  return gates
    .map((g) => {
      if (g.kind === "Rx" || g.kind === "Ry" || g.kind === "Rz") {
        return `${g.kind}(${angleToString(g.angle ?? 0)})`;
      }
      // A fixed gate's kind is already its canonical token (e.g. `S'`).
      return g.kind;
    })
    .join(" ");
}

// --- Compact URL form --------------------------------------------------

// The compact, URL-safe, self-delimiting form uses a single char per fixed
// gate (see each entry's `url` in `FIXED_GATE_TABLE`: uppercase = base gate,
// lowercase s/t/v = adjoints) and lowercase x/y/z + a number for rotations.
// All those chars are left untouched by `URLSearchParams` (which only
// preserves [A-Za-z0-9_.*-]).

/** Encode gate tokens to the compact URL form (e.g. `Hx1.5708VXs`). */
export function encodeGatesUrl(gates: Gate[]): string {
  return gates
    .map((g) => {
      if (g.kind === "Rx" || g.kind === "Ry" || g.kind === "Rz") {
        return ROTATION_TABLE[g.kind].sub + angleToString(g.angle ?? 0);
      }
      return FIXED_TO_URL[g.kind];
    })
    .join("");
}

// Matches one compact token: a rotation (lowercase x/y/z + number) or a
// single fixed-gate character. The fixed-char class is derived from the
// table's `url` codes so it stays in sync with the gate set.
const URL_FIXED_CHARS = FIXED_GATE_KINDS.map((k) => FIXED_TO_URL[k]).join("");
const URL_TOKEN_RE = new RegExp(
  `([xyz])(${NUMBER_SRC})|([${URL_FIXED_CHARS}])`,
  "g",
);

/**
 * Decode the compact URL form back into gate tokens. `modified` reports
 * whether unrecognized characters were skipped or the sequence was capped.
 */
export function decodeGatesUrl(input: string | undefined | null): {
  gates: Gate[];
  modified: boolean;
} {
  if (!input) return { gates: [], modified: false };
  const gates: Gate[] = [];
  let consumed = 0;
  for (const m of input.matchAll(URL_TOKEN_RE)) {
    consumed += m[0].length;
    if (m[1]) {
      const angle = Number.parseFloat(m[2]);
      if (Number.isFinite(angle)) {
        gates.push({
          kind: ("R" + m[1].toUpperCase()) as RotationGateKind,
          angle,
        });
      }
    } else if (m[3]) {
      gates.push({ kind: URL_TO_FIXED[m[3]] });
    }
  }
  const capped = gates.slice(0, MAX_GATE_SEQUENCE_LENGTH);
  return {
    gates: capped,
    modified: consumed !== input.length || capped.length !== gates.length,
  };
}

// Re-exported so consumers that only need the complex-number type for
// matrix rendering don't reach past this module.
export { Cplx };
