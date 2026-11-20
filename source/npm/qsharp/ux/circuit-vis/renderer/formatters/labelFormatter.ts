// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import type { DataAttributes } from "../../data/circuit.js";
import { mathChars } from "../../utils.js";
import { createSvgElement, text } from "./formatUtils.js";

/**
 * What a label is in the circuit, which decides how it is drawn:
 *
 * - `gate`: the name in a gate box, such as `H`
 * - `argument`: the argument under a gate name, such as `π/2`
 * - `ket`: the state in a ket box, such as `|0⟩`
 * - `basis`: the basis letter in a measurement box
 * - `group`: the name at the top of an expanded group
 * - `control`: the result in a classical control circle, such as `c₀`
 * - `qubit`: the qubit at the start of a wire, such as `|ψ₀⟩`
 */
export type LabelKind =
  | "gate"
  | "argument"
  | "ket"
  | "basis"
  | "group"
  | "control"
  | "qubit";

/**
 * How a run of label text is drawn:
 *
 * - `plain`: upright, in the main font
 * - `italic`: in the math font
 * - `subscript`: smaller and below the baseline
 * - `dagger`: smaller and raised, after an adjoint's name
 */
export type LabelStyle = "plain" | "italic" | "subscript" | "dagger";

/** Label text drawn in one style. */
export type LabelRun = { text: string; style: LabelStyle };

/** Rendering data for a label: what it is, where it goes, and its text as styled runs. */
export interface LabelRenderData {
  kind: LabelKind;
  /** x coord of the label. */
  x: number;
  /** y coord of the middle of the label. */
  y: number;
  fontSize: number;
  runs: LabelRun[];
  /** Custom data attributes to attach to the label element. */
  dataAttributes?: DataAttributes;
}

/** Classes the circuit stylesheet uses to style each kind of label. */
const labelClasses: Record<LabelKind, string> = {
  gate: "qs-maintext",
  // Without qs-maintext, upright argument text uses the page font on screen.
  argument: "arg-button",
  ket: "qs-maintext ket-text",
  basis: "measurement-basis",
  group: "qs-maintext qs-group-label",
  control: "qs-maintext",
  qubit: "qs-maintext qs-qubit-label",
};

/** Attributes of the tspan that draws each style other than `plain`. */
const runAttributes: Record<
  Exclude<LabelStyle, "plain">,
  { [attr: string]: string }
> = {
  italic: { class: "qs-mathtext" },
  subscript: { "baseline-shift": "sub", "font-size": "65%" },
  dagger: { dx: "2", dy: "-3", style: "font-size: 0.8em;" },
};

/**
 * Splits text that follows gate-name conventions into styled runs: Latin and Greek letters are
 * italic, `_` followed by digits is a subscript of those digits, and a trailing `'` (adjoint)
 * becomes a dagger. Other characters, including digits, stay plain.
 *
 * @param label Gate name, gate argument, or other text written like a gate name.
 *
 * @returns Styled runs of the label.
 */
export function gateLabelRuns(label: string): LabelRun[] {
  const runs: LabelRun[] = [];
  const isAdjoint = label.endsWith("'");
  const name = isAdjoint ? label.slice(0, -1) : label;
  let offset = 0;
  // Letters include the whole Greek and Coptic block (https://unicodeplus.com/block/0370).
  for (const match of name.matchAll(/_(\d+)|[a-zA-Z\u{0370}-\u{03ff}]+/gu)) {
    if (match.index > offset) {
      runs.push({ text: name.slice(offset, match.index), style: "plain" });
    }
    runs.push(
      match[1] === undefined
        ? { text: match[0], style: "italic" }
        : { text: match[1], style: "subscript" },
    );
    offset = match.index + match[0].length;
  }
  if (offset < name.length) {
    runs.push({ text: name.slice(offset), style: "plain" });
  }
  if (isAdjoint) {
    runs.push({ text: mathChars.dagger, style: "dagger" });
  }
  return runs;
}

/**
 * Draws a label as SVG text for the interactive view.
 *
 * @param label Rendering data of the label.
 *
 * @returns SVG text element for the label.
 */
export function formatLabel(label: LabelRenderData): SVGTextElement {
  const el = text("", label.x, label.y, label.fontSize);
  if (label.kind === "control" || label.kind === "qubit") {
    el.setAttribute("text-anchor", "start");
    el.setAttribute("dominant-baseline", "middle");
  }
  for (const [attr, val] of Object.entries(label.dataAttributes ?? {})) {
    el.setAttribute(`data-${attr}`, val);
  }
  el.setAttribute("class", labelClasses[label.kind]);
  el.append(...label.runs.map(formatRun));
  return el;
}

function formatRun(run: LabelRun): Node {
  if (run.style === "plain") {
    return document.createTextNode(run.text);
  }
  const tspan = createSvgElement("tspan", runAttributes[run.style]);
  tspan.append(run.text);
  return tspan;
}
