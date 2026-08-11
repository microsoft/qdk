// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { GateRenderData, GateType } from "./gateRenderData.js";
import {
  minGateWidth,
  labelPaddingX,
  labelFontSize,
  argsFontSize,
  controlCircleOffset,
} from "./constants.js";

/**
 * Calculate the width of a gate, given its render data.
 *
 * @param renderData - The rendering data of the gate, including its type, label, display arguments.
 *
 * @returns Width of given gate (in pixels).
 */
const getMinGateWidth = ({
  type,
  label,
  displayArgs,
  classicalControlIds,
}: GateRenderData): number => {
  switch (type) {
    case GateType.Measure:
    case GateType.Cnot:
    case GateType.Swap:
      return minGateWidth;
    default: {
      // Classically controlled gates are wider because of the control button on the left
      const controlButtonWidth =
        classicalControlIds != null ? controlCircleOffset : 0;
      const labelWidth = _getStringWidth(label);
      const argsWidth =
        displayArgs != null ? _getStringWidth(displayArgs, argsFontSize) : 0;
      const textWidth = Math.max(labelWidth, argsWidth) + labelPaddingX * 2;
      return Math.max(minGateWidth, textWidth) + controlButtonWidth;
    }
  }
};

/**
 * Estimate string width in pixels based on character types and font size. This may not match the
 * true rendered width, but should be close enough for calculating layout.
 *
 * @param text - The text string to measure.
 * @param fontSize - The font size in pixels (default is labelFontSize).
 *
 * @returns Estimated width of the string in pixels.
 */
const _getStringWidth = (
  text: string,
  fontSize: number = labelFontSize,
): number => {
  let units = 0;
  for (const ch of Array.from(text)) {
    if (ch === " ") {
      units += 0.33;
      continue;
    }
    if ("il.:;,'`!|".includes(ch)) {
      units += 0.28;
      continue;
    }
    if ("mw".includes(ch)) {
      units += 0.72;
      continue;
    }
    if ("MW@#%&".includes(ch)) {
      units += 0.78;
      continue;
    }
    if (/[0-9]/.test(ch)) {
      units += 0.55;
      continue;
    }
    if (/[A-Z]/.test(ch)) {
      units += 0.56;
      continue;
    }
    if (/[a-z]/.test(ch)) {
      units += 0.5;
      continue;
    }
    if (/[θπ]/.test(ch)) {
      units += 0.56;
      continue;
    }
    if (/[ψ]/.test(ch)) {
      units += 0.6;
      continue;
    }
    if ("-+*/=^~_<>".includes(ch)) {
      units += 0.5;
      continue;
    }
    units += 0.56;
  }
  const kerningFudge = Math.max(0, text.length - 1) * 0.005;
  // Round to a whole number to keep it easy to read
  return Math.floor((units + kerningFudge) * fontSize);
};

export { getMinGateWidth };
