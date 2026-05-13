// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import {
  gateHeight,
  horizontalGap,
  minGateWidth,
  verticalGap,
} from "../renderer/constants.js";
import { formatGate } from "../renderer/formatters/gateFormatter.js";
import { toRenderData } from "./standaloneRenderData.js";
import { GateDictionary, toolboxGateDictionary } from "./toolboxGates.js";

/**
 * Build the toolbox panel: a `<div class="toolbox-panel">` holding a
 * 2-column grid of gate icons plus an optional Run button.
 *
 * The toolbox always renders. The Run button only renders when
 * `runCallback` is provided — hosts that can't run circuits (e.g.
 * read-only previews) omit the callback and get no button at all,
 * not a hidden one taking up vertical space.
 *
 * Returned element is the inner toolbox; [shell.ts](shell.ts) wraps
 * it in the outer `<div class="panel">` that sits to the left of the
 * circuit in the editor layout.
 *
 * @param runCallback   Optional Run-button click handler.
 * @returns             HTML element for the toolbox.
 */
const createToolboxElement = (runCallback?: () => void): HTMLElement => {
  // Generate gate elements in a 2-column grid
  let prefixX = 0;
  let prefixY = 0;
  const gateElems = Object.keys(toolboxGateDictionary).map((key, index) => {
    const { width: gateWidth } = toRenderData(toolboxGateDictionary[key], 0, 0);

    // Reset prefixX every 2 gates and start a new row
    if (index % 2 === 0 && index !== 0) {
      prefixX = 0;
      prefixY += gateHeight + verticalGap;
    }

    const gateElem = _gate(
      toolboxGateDictionary,
      key.toString(),
      prefixX,
      prefixY,
    );
    prefixX += gateWidth + horizontalGap;
    return gateElem;
  });

  // Generate svg container to store gate elements
  const svgElem = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svgElem.classList.add("toolbox-panel-svg");
  _childrenSvg(svgElem, gateElems);

  // Append run button only when the host provided a click handler.
  // Hosts that can't run circuits omit the callback and get no button.
  let totalSvgHeight: number;
  if (runCallback != null) {
    const runButtonGroup = _createRunButton(
      prefixY + gateHeight + 20,
      runCallback,
    );
    svgElem.appendChild(runButtonGroup);
    totalSvgHeight = prefixY + 2 * gateHeight + 32; // gates + button + padding
  } else {
    totalSvgHeight = prefixY + gateHeight + 16; // gates + padding (no button)
  }

  // Size SVG to content height so the toolbox panel can scroll when window is short
  svgElem.setAttribute("height", totalSvgHeight.toString());
  svgElem.setAttribute("width", "100%");

  // Generate toolbox panel
  const toolboxElem = _elem("div", "toolbox-panel");
  _children(toolboxElem, [_title("Toolbox")]);
  toolboxElem.appendChild(svgElem);

  return toolboxElem;
};

/**
 * Build the Run button. Created visible and pre-wired — callers only
 * get this far if they actually want a Run button.
 *
 * @param buttonY      Y coordinate for the top of the button.
 * @param onClick      Click handler.
 * @returns            SVG group element containing the run button.
 */
const _createRunButton = (
  buttonY: number,
  onClick: () => void,
): SVGGElement => {
  const buttonWidth = minGateWidth * 2 + horizontalGap;
  const buttonHeight = gateHeight;
  const buttonX = 1;

  const runButtonGroup = document.createElementNS(
    "http://www.w3.org/2000/svg",
    "g",
  );
  runButtonGroup.setAttribute("class", "svg-run-button");
  runButtonGroup.setAttribute("tabindex", "0");
  runButtonGroup.setAttribute("role", "button");

  // Rectangle background
  const rect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
  rect.setAttribute("x", buttonX.toString());
  rect.setAttribute("y", buttonY.toString());
  rect.setAttribute("width", buttonWidth.toString());
  rect.setAttribute("height", buttonHeight.toString());
  rect.setAttribute("class", "svg-run-button-rect");

  // Text label
  const text = document.createElementNS("http://www.w3.org/2000/svg", "text");
  text.setAttribute("x", (buttonX + buttonWidth / 2).toString());
  text.setAttribute("y", (buttonY + buttonHeight / 2).toString());
  text.setAttribute("class", "svg-run-button-text");
  text.textContent = "Run";

  runButtonGroup.appendChild(rect);
  runButtonGroup.appendChild(text);

  runButtonGroup.addEventListener("click", onClick);
  return runButtonGroup;
};

/**
 * Build a single toolbox gate icon by routing the toolbox's prototype
 * `Operation` through the same gate formatter the main render path
 * uses, so toolbox icons stay visually in lockstep with their
 * dropped-on-circuit counterparts.
 *
 * @param gateDictionary - The dictionary containing gate operations.
 * @param type - The toolbox key. Example: `"H"` or `"X"`.
 * @param x - The x coordinate at the starting point from the left.
 * @param y - The y coordinate at the starting point from the top.
 * @returns The generated SVG element representing the gate.
 * @throws Will throw an error if the gate type is not available in the dictionary.
 */
const _gate = (
  gateDictionary: GateDictionary,
  type: string,
  x: number,
  y: number,
): SVGElement => {
  const gate = gateDictionary[type];
  if (gate == null) throw new Error(`Gate ${type} not available`);
  const renderData = toRenderData(gate, x, y);
  renderData.dataAttributes = { type: type };
  const gateElem = formatGate(renderData).cloneNode(true) as SVGElement;
  gateElem.setAttribute("toolbox-item", "true");

  return gateElem;
};

/* ----- small DOM helpers, private to this file ----- */

const _elem = (tag: string, className?: string): HTMLElement => {
  const elem = document.createElement(tag);
  if (className) {
    elem.className = className;
  }
  return elem;
};

const _children = (
  parentElem: HTMLElement,
  childElems: HTMLElement[],
): HTMLElement => {
  childElems.map((elem) => parentElem.appendChild(elem));
  return parentElem;
};

const _childrenSvg = (
  parentElem: SVGElement,
  childElems: SVGElement[],
): SVGElement => {
  childElems.map((elem) => parentElem.appendChild(elem));
  return parentElem;
};

const _title = (text: string): HTMLElement => {
  const titleElem = _elem("h2");
  titleElem.className = "title";
  titleElem.textContent = text;
  return titleElem;
};

export { createToolboxElement };
