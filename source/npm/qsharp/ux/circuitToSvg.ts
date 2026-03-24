// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/**
 * Pure-string SVG renderer for quantum circuits.
 *
 * Produces a standalone `<svg>` string from a CircuitGroup / Circuit object
 * with no DOM dependency.  Designed for static export (reports, papers).
 *
 * Supports a `gatesPerRow` option that wraps the circuit into multiple rows
 * so wide circuits fit within a target page width.
 */

import {
  toCircuitGroup,
  type CircuitGroup,
  type Circuit,
  type Column,
  type Operation,
  type Qubit,
} from "./circuit-vis/circuit.js";
import type { Register } from "./circuit-vis/register.js";

// ── Layout constants (matching circuit-vis/constants.ts) ────────────────

const GATE_HEIGHT = 40;
const MIN_GATE_WIDTH = 40;
const GATE_PAD = 6;
const LABEL_FONT = 14;
const ARGS_FONT = 12;
const START_X = 80; // space for qubit labels
const START_Y = 40;
const WIRE_END_PAD = 20;
const CONTROL_DOT_R = 5;
const OPLUS_R = 18;
const MEAS_W = 40;
const MEAS_H = 40;
const KET_W = 40;
const ROW_GAP = 30; // vertical gap between wrapped rows

// ── Helpers ─────────────────────────────────────────────────────────────

function esc(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function attrs(a: Record<string, string | number>): string {
  return Object.entries(a)
    .map(([k, v]) => `${k}="${esc(String(v))}"`)
    .join(" ");
}

/** Approximate text width for label sizing (sans-serif ~0.6em per char). */
function textWidth(s: string, fontSize: number): number {
  return s.length * fontSize * 0.6;
}

/** Compute the display width of a gate given its label and args. */
function gateWidth(label: string, displayArgs?: string): number {
  let w = textWidth(label, LABEL_FONT) + 20;
  if (displayArgs) {
    w = Math.max(w, textWidth(displayArgs, ARGS_FONT) + 20);
  }
  return Math.max(MIN_GATE_WIDTH, Math.ceil(w));
}

// ── Math characters ─────────────────────────────────────────────────────

const MATH = {
  psi: "\u03c8", // ψ
  rangle: "\u27e9", // ⟩
  dagger: "\u2020", // †
};

// ── Qubit label rendering ───────────────────────────────────────────────

function qubitLabel(qid: number, y: number): string {
  // |ψ₀⟩  style label
  const sub = String(qid)
    .split("")
    .map((c) => String.fromCharCode(0x2080 + Number(c)))
    .join("");
  const label = `|${MATH.psi}${sub}${MATH.rangle}`;
  return `<text ${attrs({ x: START_X - 15, y, "font-size": LABEL_FONT, "text-anchor": "end", "dominant-baseline": "middle", class: "qs-qubit-label qs-maintext" })}>${esc(label)}</text>`;
}

// ── Gate SVG fragments ──────────────────────────────────────────────────

function unitaryBox(
  cx: number,
  ys: number[],
  label: string,
  w: number,
  displayArgs?: string,
): string {
  const topY = Math.min(...ys);
  const bottomY = Math.max(...ys);
  const h = Math.max(GATE_HEIGHT, bottomY - topY + GATE_HEIGHT);
  const bx = cx - w / 2;
  const by = (topY + bottomY) / 2 - h / 2;
  let s = `<rect ${attrs({ x: bx, y: by, width: w, height: h, class: "gate-unitary" })} />`;
  s += `<text ${attrs({ x: cx, y: (topY + bottomY) / 2 - (displayArgs ? 6 : 0), "font-size": LABEL_FONT, "text-anchor": "middle", "dominant-baseline": "middle", class: "qs-maintext" })}>${esc(label)}</text>`;
  if (displayArgs) {
    s += `<text ${attrs({ x: cx, y: (topY + bottomY) / 2 + 10, "font-size": ARGS_FONT, "text-anchor": "middle", "dominant-baseline": "middle" })}>${esc(displayArgs)}</text>`;
  }
  return s;
}

function measureBox(cx: number, y: number): string {
  const bx = cx - MEAS_W / 2;
  const by = y - MEAS_H / 2;
  let s = `<rect ${attrs({ x: bx, y: by, width: MEAS_W, height: MEAS_H, class: "gate-measure" })} />`;
  // Arc
  const arcRx = MEAS_W * 0.3;
  const arcRy = MEAS_H * 0.3;
  s += `<path ${attrs({ d: `M${cx - arcRx},${y + 2} A${arcRx},${arcRy} 0 0,1 ${cx + arcRx},${y + 2}`, class: "arc-measure" })} />`;
  // Arrow
  s += `<line ${attrs({ x1: cx, y1: y + 2, x2: cx + arcRx * 0.7, y2: y - arcRy * 0.9, class: "qs-line-measure" })} />`;
  return s;
}

function ketBox(cx: number, y: number, label: string): string {
  const bx = cx - KET_W / 2;
  const by = y - GATE_HEIGHT / 2;
  let s = `<rect ${attrs({ x: bx, y: by, width: KET_W, height: GATE_HEIGHT, class: "gate-ket" })} />`;
  s += `<text ${attrs({ x: cx, y, "font-size": LABEL_FONT, "text-anchor": "middle", "dominant-baseline": "middle", class: "ket-text qs-maintext" })}>${esc(label)}</text>`;
  return s;
}

function controlDot(cx: number, y: number): string {
  return `<circle ${attrs({ cx, cy: y, r: CONTROL_DOT_R, class: "control-dot" })} />`;
}

function controlLine(cx: number, y1: number, y2: number): string {
  return `<line ${attrs({ x1: cx, y1, x2: cx, y2, class: "control-line" })} />`;
}

function oplusGate(cx: number, y: number): string {
  const r = OPLUS_R;
  let s = `<g class="oplus">`;
  s += `<circle ${attrs({ cx, cy: y, r })} />`;
  s += `<line ${attrs({ x1: cx - r, y1: y, x2: cx + r, y2: y })} />`;
  s += `<line ${attrs({ x1: cx, y1: y - r, x2: cx, y2: y + r })} />`;
  s += `</g>`;
  return s;
}

function swapCross(cx: number, y: number): string {
  const d = 8;
  let s = `<line ${attrs({ x1: cx - d, y1: y - d, x2: cx + d, y2: y + d })} />`;
  s += `<line ${attrs({ x1: cx - d, y1: y + d, x2: cx + d, y2: y - d })} />`;
  return s;
}

// ── Group expansion ─────────────────────────────────────────────────────

/** Expand grouped operations to the specified depth.
 *  At depth 0, groups are left as-is (rendered as a single box).
 *  At depth 1+, the group's children columns are inlined. */
function expandGrid(grid: Column[], depth: number): Column[] {
  if (depth <= 0) return grid;

  const result: Column[] = [];
  for (const col of grid) {
    const expandedComponents: Operation[] = [];
    let inlinedColumns: Column[] | null = null;

    for (const op of col.components) {
      if (op.children && op.children.length > 0) {
        // This operation is a group — expand it
        const childGrid = expandGrid(op.children, depth - 1);
        if (inlinedColumns === null) {
          inlinedColumns = childGrid;
        } else {
          // Merge: pad the shorter one with empty columns
          for (let i = 0; i < childGrid.length; i++) {
            if (i < inlinedColumns.length) {
              inlinedColumns[i] = {
                components: [
                  ...inlinedColumns[i].components,
                  ...childGrid[i].components,
                ],
              };
            } else {
              inlinedColumns.push(childGrid[i]);
            }
          }
        }
      } else {
        expandedComponents.push(op);
      }
    }

    if (inlinedColumns) {
      // If there are also non-group ops in this column, prepend them
      // to the first inlined column
      if (expandedComponents.length > 0) {
        inlinedColumns[0] = {
          components: [...expandedComponents, ...inlinedColumns[0].components],
        };
      }
      result.push(...inlinedColumns);
    } else {
      result.push(col);
    }
  }
  return result;
}

// ── Column width computation ────────────────────────────────────────────

interface QubitPos {
  id: number;
  y: number;
}

function computeQubitPositions(qubits: Qubit[], offsetY: number): QubitPos[] {
  let y = offsetY + START_Y + GATE_PAD + GATE_HEIGHT / 2;
  return qubits.map((q) => {
    const pos = { id: q.id, y };
    // Advance by gate+pad for each qubit row, plus classical results
    const numClassical = q.numResults ?? 0;
    y += GATE_HEIGHT + GATE_PAD * 2;
    y += numClassical * (GATE_HEIGHT / 2 + GATE_PAD);
    return pos;
  });
}

function qubitY(positions: QubitPos[], qubitId: number): number {
  const q = positions.find((p) => p.id === qubitId);
  return q ? q.y : 0;
}

function operationWidth(op: Operation): number {
  switch (op.kind) {
    case "measurement":
      return MEAS_W;
    case "ket":
      return KET_W;
    case "unitary": {
      if (op.gate === "CNOT" || op.gate === "CX" || op.gate === "X") {
        // Check if it's a controlled-X (rendered as CNOT dot+oplus)
        if (op.controls && op.controls.length > 0 && op.gate === "X") {
          return OPLUS_R * 2;
        }
        if (op.gate === "CNOT" || op.gate === "CX") {
          return OPLUS_R * 2;
        }
      }
      if (op.gate === "SWAP") return MIN_GATE_WIDTH;
      const argStr = op.args?.join(", ");
      return gateWidth(op.gate + (op.isAdjoint ? MATH.dagger : ""), argStr);
    }
    default:
      return MIN_GATE_WIDTH;
  }
}

function columnWidth(col: Column): number {
  let maxW = MIN_GATE_WIDTH;
  for (const op of col.components) {
    maxW = Math.max(maxW, operationWidth(op));
  }
  return maxW;
}

// ── Bounding box tracking for operations ────────────────────────────────

function trackOperationBB(
  op: Operation,
  cx: number,
  positions: QubitPos[],
  extendBB: (cx: number, cy: number, hw: number, hh: number) => void,
) {
  switch (op.kind) {
    case "measurement": {
      const qy = qubitY(positions, op.qubits[0].qubit);
      extendBB(cx, qy, MEAS_W / 2 + 2, MEAS_H / 2 + 2);
      break;
    }
    case "ket": {
      const qy = qubitY(positions, op.targets[0].qubit);
      extendBB(cx, qy, KET_W / 2 + 2, GATE_HEIGHT / 2 + 2);
      break;
    }
    case "unitary": {
      const targetYs = op.targets.map((t: Register) =>
        qubitY(positions, t.qubit),
      );
      const controlYs = (op.controls ?? []).map((c: Register) =>
        qubitY(positions, c.qubit),
      );
      const allYs = [...targetYs, ...controlYs];
      const minY = Math.min(...allYs);
      const maxY = Math.max(...allYs);

      // CNOT / controlled-X
      if (
        (op.gate === "CNOT" ||
          op.gate === "CX" ||
          (op.gate === "X" && (op.controls?.length ?? 0) > 0)) &&
        targetYs.length === 1
      ) {
        extendBB(cx, targetYs[0], OPLUS_R + 2, OPLUS_R + 2);
        for (const cy of controlYs)
          extendBB(cx, cy, CONTROL_DOT_R + 2, CONTROL_DOT_R + 2);
        break;
      }

      // SWAP
      if (op.gate === "SWAP" && targetYs.length === 2) {
        for (const ty of targetYs) extendBB(cx, ty, 12, 12);
        for (const cy of controlYs)
          extendBB(cx, cy, CONTROL_DOT_R + 2, CONTROL_DOT_R + 2);
        break;
      }

      // Regular unitary box
      const argStr = op.args?.join(", ");
      const w = gateWidth(op.gate + (op.isAdjoint ? MATH.dagger : ""), argStr);
      const h = Math.max(GATE_HEIGHT, maxY - minY + GATE_HEIGHT);
      extendBB(cx, (minY + maxY) / 2, w / 2 + 2, h / 2 + 2);
      for (const cy of controlYs)
        extendBB(cx, cy, CONTROL_DOT_R + 2, CONTROL_DOT_R + 2);
      break;
    }
  }
}

// ── Render one operation ────────────────────────────────────────────────

function renderOperation(
  op: Operation,
  cx: number,
  positions: QubitPos[],
): string {
  let svg = "";

  switch (op.kind) {
    case "measurement": {
      const qy = qubitY(positions, op.qubits[0].qubit);
      svg += measureBox(cx, qy);
      break;
    }
    case "ket": {
      const qy = qubitY(positions, op.targets[0].qubit);
      svg += ketBox(cx, qy, op.gate);
      break;
    }
    case "unitary": {
      const label = op.gate + (op.isAdjoint ? MATH.dagger : "");
      const argStr = op.args?.join(", ");
      const targetYs = op.targets.map((t: Register) =>
        qubitY(positions, t.qubit),
      );
      const controls = op.controls ?? [];
      const controlYs = controls.map((c: Register) =>
        qubitY(positions, c.qubit),
      );

      const allYs = [...targetYs, ...controlYs];
      const minY = Math.min(...allYs);
      const maxY = Math.max(...allYs);

      // Handle special gates
      if (
        (op.gate === "CNOT" || op.gate === "CX") &&
        targetYs.length === 1 &&
        controlYs.length >= 1
      ) {
        // CNOT: control dot(s) + oplus on target
        if (minY !== maxY) svg += controlLine(cx, minY, maxY);
        for (const cy of controlYs) svg += controlDot(cx, cy);
        svg += oplusGate(cx, targetYs[0]);
        break;
      }

      if (op.gate === "X" && controls.length > 0 && targetYs.length === 1) {
        // Controlled-X rendered as CNOT
        if (minY !== maxY) svg += controlLine(cx, minY, maxY);
        for (const cy of controlYs) svg += controlDot(cx, cy);
        svg += oplusGate(cx, targetYs[0]);
        break;
      }

      if (op.gate === "SWAP" && targetYs.length === 2) {
        if (minY !== maxY) svg += controlLine(cx, minY, maxY);
        for (const cy of controlYs) svg += controlDot(cx, cy);
        svg += swapCross(cx, targetYs[0]);
        svg += swapCross(cx, targetYs[1]);
        break;
      }

      // Controlled unitary: dots + line + box on targets
      if (controls.length > 0) {
        if (minY !== maxY) svg += controlLine(cx, minY, maxY);
        for (const cy of controlYs) svg += controlDot(cx, cy);
      }

      const w = gateWidth(label, argStr);
      svg += unitaryBox(cx, targetYs, label, w, argStr);
      break;
    }
  }
  return svg;
}

// ── CSS for standalone SVG ──────────────────────────────────────────────

const CIRCUIT_CSS = `
  .qs-circuit line, .qs-circuit circle, .qs-circuit rect {
    stroke: #202020; stroke-width: 1;
  }
  .qs-circuit text {
    fill: #202020; dominant-baseline: middle; text-anchor: middle;
    font-family: "KaTeX_Main", sans-serif; user-select: none;
  }
  .qs-circuit .qs-qubit-label { text-anchor: end; }
  .qs-circuit .gate-unitary { fill: #ddd; }
  .qs-circuit .gate text { fill: #202020; }
  .qs-circuit .gate-measure { fill: #007acc; }
  .qs-circuit .arc-measure, .qs-circuit .qs-line-measure {
    stroke: #fff; fill: none; stroke-width: 1;
  }
  .qs-circuit .gate-ket { fill: #007acc; }
  .qs-circuit .ket-text { fill: #fff; stroke: none; }
  .qs-circuit .control-dot { fill: #202020; stroke: none; }
  .qs-circuit .control-line { stroke: #202020; stroke-width: 1; }
  .qs-circuit .oplus > circle { fill: #fff; stroke: #202020; stroke-width: 2; }
  .qs-circuit .oplus > line { stroke: #202020; stroke-width: 2; }
  .qs-circuit rect.gate-swap { fill: transparent; stroke: transparent; }
  .qs-circuit .register-classical { stroke-width: 0.5; }
`;

const CIRCUIT_CSS_DARK = `
  .qs-circuit line, .qs-circuit circle, .qs-circuit rect {
    stroke: #d4d4d4; stroke-width: 1;
  }
  .qs-circuit text { fill: #d4d4d4; }
  .qs-circuit .gate-unitary { fill: #333; }
  .qs-circuit .gate text { fill: #d4d4d4; }
  .qs-circuit .gate-measure { fill: #007acc; }
  .qs-circuit .arc-measure, .qs-circuit .qs-line-measure {
    stroke: #fff; fill: none; stroke-width: 1;
  }
  .qs-circuit .gate-ket { fill: #007acc; }
  .qs-circuit .ket-text { fill: #fff; stroke: none; }
  .qs-circuit .control-dot { fill: #d4d4d4; stroke: none; }
  .qs-circuit .control-line { stroke: #d4d4d4; stroke-width: 1; }
  .qs-circuit .oplus > circle { fill: #1e1e1e; stroke: #d4d4d4; stroke-width: 2; }
  .qs-circuit .oplus > line { stroke: #d4d4d4; stroke-width: 2; }
`;

// ── Main export ─────────────────────────────────────────────────────────

export interface CircuitToSvgOptions {
  /** Maximum number of gate columns per row before wrapping.  0 = no wrap. */
  gatesPerRow?: number;
  /** Use dark-mode colours. */
  darkMode?: boolean;
  /** How many levels of grouped operations to expand.
   *  0 (default) = show groups as collapsed single-gate boxes.
   *  1 = expand one level, showing children inline.
   *  Infinity = fully expand everything. */
  renderDepth?: number;
}

/**
 * Render a quantum circuit to a standalone SVG string.
 *
 * Accepts any format that `toCircuitGroup` understands (Circuit,
 * CircuitGroup, or legacy schema).  Returns a self-contained SVG with
 * embedded CSS — no external stylesheet needed.
 *
 * @param circuit  Circuit data (object or JSON string).
 * @param options  Rendering options.
 * @returns SVG markup string.
 */
export function circuitToSvg(
  circuit: CircuitGroup | Circuit | unknown,
  options: CircuitToSvgOptions = {},
): string {
  const { gatesPerRow = 0, darkMode = false, renderDepth = 0 } = options;

  // Parse if given as string
  const data = typeof circuit === "string" ? JSON.parse(circuit) : circuit;

  const result = toCircuitGroup(data);
  if (!result.ok) {
    throw new Error(`Circuit conversion error: ${result.error}`);
  }

  const cg = result.circuitGroup;
  if (cg.circuits.length === 0 || !cg.circuits[0]) {
    throw new Error("No circuit found in input.");
  }

  const circ = cg.circuits[0];
  const qubits = circ.qubits ?? [];
  // Expand grouped operations to the requested depth
  const grid = expandGrid(circ.componentGrid ?? [], renderDepth);

  if (qubits.length === 0) {
    return `<svg xmlns="http://www.w3.org/2000/svg" width="200" height="40"><text x="100" y="20" text-anchor="middle" font-size="14">Empty circuit</text></svg>`;
  }

  // Split columns into rows
  const rows: Column[][] = [];
  if (gatesPerRow > 0 && grid.length > gatesPerRow) {
    for (let i = 0; i < grid.length; i += gatesPerRow) {
      rows.push(grid.slice(i, i + gatesPerRow));
    }
  } else {
    rows.push(grid);
  }

  // Compute qubit positions for a single row (relative to row origin)
  const basePositions = computeQubitPositions(qubits, 0);
  const rowHeight =
    basePositions.length > 0
      ? basePositions[basePositions.length - 1].y -
        basePositions[0].y +
        GATE_HEIGHT +
        GATE_PAD * 2
      : GATE_HEIGHT + GATE_PAD * 2;

  let totalWidth = 0;
  let totalHeight = 0;
  const rowSvgs: string[] = [];

  // Track bounding box of all rendered elements
  let bbMinX = Infinity,
    bbMinY = Infinity,
    bbMaxX = -Infinity,
    bbMaxY = -Infinity;
  function extendBB(cx: number, cy: number, hw: number, hh: number) {
    bbMinX = Math.min(bbMinX, cx - hw);
    bbMinY = Math.min(bbMinY, cy - hh);
    bbMaxX = Math.max(bbMaxX, cx + hw);
    bbMaxY = Math.max(bbMaxY, cy + hh);
  }

  for (let rowIdx = 0; rowIdx < rows.length; rowIdx++) {
    const rowCols = rows[rowIdx];
    const rowOffsetY = rowIdx * (rowHeight + ROW_GAP);

    // Qubit positions for this row
    const positions = computeQubitPositions(qubits, rowOffsetY);

    // Compute column x-positions
    const colWidths = rowCols.map(columnWidth);
    const colXs: number[] = [];
    let x = START_X + GATE_PAD;
    for (let i = 0; i < rowCols.length; i++) {
      x += colWidths[i] / 2 + GATE_PAD;
      colXs.push(x);
      x += colWidths[i] / 2 + GATE_PAD;
    }
    const wireEndX = x + WIRE_END_PAD;

    // Qubit labels (only first row if wrapping)
    let rowSvg = "";
    if (rowIdx === 0 || gatesPerRow > 0) {
      for (const pos of positions) {
        rowSvg += qubitLabel(pos.id, pos.y);
        // Label text extends left; approximate width
        extendBB(START_X - 15, pos.y, 60, LABEL_FONT / 2 + 2);
      }
    }

    // Horizontal wires
    const wireStartX = START_X - 10;
    for (const pos of positions) {
      rowSvg += `<line ${attrs({ x1: wireStartX, y1: pos.y, x2: wireEndX, y2: pos.y })} />`;
      extendBB(
        (wireStartX + wireEndX) / 2,
        pos.y,
        (wireEndX - wireStartX) / 2,
        1,
      );
    }

    // Gates
    for (let ci = 0; ci < rowCols.length; ci++) {
      const col = rowCols[ci];
      const cx = colXs[ci];
      for (const op of col.components) {
        rowSvg += `<g class="gate">${renderOperation(op, cx, positions)}</g>`;
        // Track bounding box for each operation
        trackOperationBB(op, cx, positions, extendBB);
      }
    }

    rowSvgs.push(rowSvg);
    totalWidth = Math.max(totalWidth, wireEndX);
    totalHeight = rowOffsetY + rowHeight;
  }

  // Build final SVG with exact bounding box + small padding
  const pad = 5;
  if (bbMinX === Infinity) {
    bbMinX = 0;
    bbMinY = 0;
    bbMaxX = totalWidth;
    bbMaxY = totalHeight;
  }
  const vbX = bbMinX - pad;
  const vbY = bbMinY - pad;
  const vbW = bbMaxX - bbMinX + pad * 2;
  const vbH = bbMaxY - bbMinY + pad * 2;
  const css = darkMode ? CIRCUIT_CSS_DARK : CIRCUIT_CSS;

  let svg = `<svg xmlns="http://www.w3.org/2000/svg" class="qs-circuit" width="${vbW.toFixed(0)}" height="${vbH.toFixed(0)}" viewBox="${vbX.toFixed(1)} ${vbY.toFixed(1)} ${vbW.toFixed(1)} ${vbH.toFixed(1)}">`;
  svg += `<defs><style>${css}</style></defs>`;
  for (const rowSvg of rowSvgs) {
    svg += rowSvg;
  }
  svg += `</svg>`;

  return svg;
}
