// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { GateRenderData, GateType } from "../gateRenderData.js";
import {
  minGateWidth,
  gateHeight,
  labelFontSize,
  argsFontSize,
  controlBtnRadius,
  controlBtnOffset,
  groupBoxPadding,
  classicalRegHeight,
  nestedGroupPadding,
} from "../constants.js";
import {
  createSvgElement,
  group,
  line,
  circle,
  controlDot,
  box,
  text,
  arc,
  dashedLine,
  dashedBox,
} from "./formatUtils.js";

import { mathChars } from "../utils.js";

/**
 * Given an array of operations render data, return the SVG representation.
 *
 * @param renderData 2D array of rendering data for gates.
 * @param nestedDepth Depth of nested operations (used in classically controlled and grouped operations).
 *
 * @returns SVG representation of operations.
 */
const formatGates = (
  renderData: GateRenderData[][],
  nestedDepth = 0,
): SVGElement => {
  const formattedGates: SVGElement[] = renderData
    .map((col) => col.map((renderData) => formatGate(renderData, nestedDepth)))
    .flat();
  return group(formattedGates);
};

/**
 * Takes in an operation's rendering data and formats it into SVG.
 *
 * @param renderData The rendering data of the gate.
 * @param nestedDepth Depth of nested operations (used in classically controlled and grouped operations).
 *
 * @returns SVG representation of gate.
 */
const formatGate = (
  renderData: GateRenderData,
  nestedDepth = 0,
): SVGElement => {
  const { type, x, controlsY, targetsY, label, displayArgs, width } =
    renderData;
  switch (type) {
    case GateType.Measure:
      return _createGate([_measure(x, controlsY[0])], renderData, nestedDepth);
    case GateType.Unitary:
      return _createGate(
        [_unitary(label, x, targetsY as number[][], width, displayArgs)],
        renderData,
        nestedDepth,
      );
    case GateType.X:
      return _createGate([_x(renderData)], renderData, nestedDepth);
    case GateType.Ket:
      return _createGate([_ket(label, renderData)], renderData, nestedDepth);
    case GateType.Swap:
      return controlsY.length > 0
        ? _controlledGate(renderData, nestedDepth)
        : _createGate(
            [_swap(renderData, nestedDepth)],
            renderData,
            nestedDepth,
          );
    case GateType.Cnot:
    case GateType.ControlledUnitary:
      return _controlledGate(renderData, nestedDepth);
    case GateType.Group:
      return _groupedOperations(renderData, nestedDepth);
    case GateType.ClassicalControlled:
      return _classicalControlled(renderData);
    default:
      throw new Error(`ERROR: unknown gate (${label}) of type ${type}.`);
  }
};

/**
 * Groups SVG elements into a gate SVG group.
 *
 * @param svgElems - Array of SVG elements that make up the gate.
 * @param renderData - Render data containing information about the gate, such as data attributes.
 * @param nestedDepth - Depth of nested operation.
 *
 * @returns SVG representation of a gate.
 */
const _createGate = (
  svgElems: SVGElement[],
  renderData: GateRenderData,
  nestedDepth: number,
): SVGElement => {
  const { dataAttributes } = renderData || {};
  const attributes: { [attr: string]: string } = { class: "gate" };
  Object.entries(dataAttributes || {}).forEach(
    ([attr, val]) => (attributes[`data-${attr}`] = val),
  );

  // Add positioning data attributes to avoid relying on getBBox() in tests
  const [x1, y1, x2, y2] = _gatePosition(renderData, nestedDepth);
  attributes["data-x"] = x1.toString();
  attributes["data-y"] = y1.toString();
  attributes["data-width"] = (x2 - x1).toString();
  attributes["data-height"] = (y2 - y1).toString();
  attributes["data-min-y"] = y1.toString();
  attributes["data-max-y"] = y2.toString();

  const location =
    renderData.dataAttributes?.sourceLinkTitle &&
    renderData.dataAttributes?.sourceLinkHref
      ? {
          title: renderData.dataAttributes?.sourceLinkTitle,
          href: renderData.dataAttributes?.sourceLinkHref,
        }
      : undefined;

  const zoomBtn: SVGElement | null = _zoomButton(renderData, nestedDepth);
  if (zoomBtn != null) svgElems = svgElems.concat([zoomBtn]);

  const gateElem = group(svgElems, attributes);

  // If there's a source location, wrap the gate in an SVG <a> element to make it clickable
  if (location) {
    const linkElem = createSvgElement("a", {
      href: location.href,
      class: "qs-circuit-source-link",
    });

    // Add title as a child <title> element for accessibility and hover tooltip
    const titleElem = createSvgElement("title");
    titleElem.textContent = location.title;
    linkElem.appendChild(titleElem);

    // Add the gate as a child of the link
    linkElem.appendChild(gateElem);
    return linkElem;
  }

  return gateElem;
};

/**
 * Returns the expand/collapse button for an operation if it can be zoomed-in or zoomed-out,
 * respectively. If neither are allowed, return `null`.
 *
 * @param renderData Operation render data.
 * @param nestedDepth Depth of nested operation.
 *
 * @returns SVG element for expand/collapse button if needed, or null otherwise.
 */
const _zoomButton = (
  renderData: GateRenderData,
  nestedDepth: number,
): SVGElement | null => {
  if (renderData == undefined) return null;

  const [x1, y1] = _gatePosition(renderData, nestedDepth);
  let { dataAttributes } = renderData;
  dataAttributes = dataAttributes || {};

  const expanded = "expanded" in dataAttributes;

  const x = x1 + 2;
  const y = y1 + 2;
  const circleBorder: SVGElement = circle(x, y, 10);

  if (expanded) {
    // Create collapse button if expanded
    const minusSign: SVGElement = createSvgElement("path", {
      d: `M${x - 7},${y} h14`,
    });
    const elements: SVGElement[] = [circleBorder, minusSign];
    return group(elements, { class: "gate-control gate-collapse" });
  } else if (dataAttributes["zoom-in"] == "true") {
    // Create expand button if operation can be zoomed in
    const plusSign: SVGElement = createSvgElement("path", {
      d: `M${x},${y - 7} v14 M${x - 7},${y} h14`,
    });
    const elements: SVGElement[] = [circleBorder, plusSign];
    return group(elements, { class: "gate-control gate-expand" });
  }

  return null;
};

/**
 * Calculate position of gate.
 *
 * @param renderData Operation render data.
 * @param nestedDepth Depth of nested operations.
 *
 * @returns Coordinates of gate: [x1, y1, x2, y2].
 */
const _gatePosition = (
  renderData: GateRenderData,
  nestedDepth: number,
): [number, number, number, number] => {
  const { x, width, type, targetsY } = renderData;

  const ys = targetsY?.flatMap((y) => y as number[]) || [];
  const maxY = Math.max(...ys);
  const minY = Math.min(...ys);

  let x1: number, y1: number, x2: number, y2: number;

  switch (type) {
    case GateType.Group: {
      const padding = groupBoxPadding - nestedDepth * nestedGroupPadding;

      x1 = x - 2 * padding;
      y1 = minY - gateHeight / 2 - padding;
      x2 = width + 2 * padding;
      y2 = maxY + +gateHeight / 2 + padding - (minY - gateHeight / 2 - padding);

      return [x1, y1, x2, y2];
    }

    default:
      x1 = x - width / 2;
      y1 = minY - gateHeight / 2;
      x2 = x + width;
      y2 = maxY + gateHeight / 2;
  }

  return [x1, y1, x2, y2];
};

/**
 * Creates a measurement gate at position (x, y).
 *
 * @param x  x coord of measurement gate.
 * @param y  y coord of measurement gate.
 *
 * @returns SVG representation of measurement gate.
 */
const _measure = (x: number, y: number): SVGElement => {
  x -= minGateWidth / 2;
  const width: number = minGateWidth,
    height = gateHeight;
  // Draw measurement box
  const mBox: SVGElement = box(
    x,
    y - height / 2,
    width,
    height,
    "gate-measure",
  );
  const mArc: SVGElement = arc(x + 5, y + 2, width / 2 - 5, height / 2 - 8);
  mArc.style.pointerEvents = "none";
  const meter: SVGElement = line(
    x + width / 2,
    y + 8,
    x + width - 8,
    y - height / 2 + 8,
  );
  meter.style.pointerEvents = "none";
  return group([mBox, mArc, meter]);
};

const use_katex = true;

function _style_gate_text(gate: SVGTextElement) {
  if (!use_katex) return;
  let label = gate.textContent || "";

  // In general, use the regular math font
  gate.classList.add("qs-maintext");

  // Wrap any latin or greek letters in tspan with KaTeX_Math font
  // Style the entire Greek + Coptic block (https://unicodeplus.com/block/0370)
  // Note this deliberately leaves ASCII digits [0-9] non-italic
  const italicChars = /[a-zA-Z\u{0370}-\u{03ff}]+/gu;

  label = label.replace(italicChars, `<tspan class='qs-mathtext'>$&</tspan>`);

  // Replace a trailing ' with the proper unicode dagger symbol
  label = label.replace(
    /'$/,
    `<tspan dx="2" dy="-3" style="font-size: 0.8em;">${mathChars.dagger}</tspan>`,
  );

  gate.innerHTML = label;
}

/**
 * Creates the SVG for a unitary gate on an arbitrary number of qubits.
 *
 * @param label            Gate label.
 * @param x                x coord of gate.
 * @param y                Array of y coords of registers acted upon by gate.
 * @param width            Width of gate.
 * @param displayArgs           Arguments passed in to gate.
 * @param params  Non-Qubit required parameters for the unitary gate.
 * @param renderDashedLine If true, draw dashed lines between non-adjacent unitaries.
 * @param cssClass         Optional CSS class to apply to the unitary gate for styling.
 *
 * @returns SVG representation of unitary gate.
 */
const _unitary = (
  label: string,
  x: number,
  y: number[][],
  width: number,
  displayArgs?: string,
  renderDashedLine = true,
  cssClass?: string,
): SVGElement => {
  if (y.length === 0)
    throw new Error(
      `Failed to render unitary gate (${label}): has no y-values`,
    );

  // Render each group as a separate unitary boxes
  const unitaryBoxes: SVGElement[] = y.map((group: number[]) => {
    const maxY: number = group[group.length - 1],
      minY: number = group[0];
    const height: number = maxY - minY + gateHeight;
    return _unitaryBox(label, x, minY, width, height, displayArgs, cssClass);
  });

  // Draw dashed line between disconnected unitaries
  if (renderDashedLine && unitaryBoxes.length > 1) {
    const lastBox: number[] = y[y.length - 1];
    const firstBox: number[] = y[0];
    const maxY: number = lastBox[lastBox.length - 1],
      minY: number = firstBox[0];
    const vertLine: SVGElement = dashedLine(x, minY, x, maxY);
    return group([vertLine, ...unitaryBoxes]);
  }

  return group(unitaryBoxes);
};

/**
 * Generates SVG representation of the boxed unitary gate symbol.
 *
 * @param label  Label for unitary operation.
 * @param x      x coord of gate.
 * @param y      y coord of gate.
 * @param width  Width of gate.
 * @param height Height of gate.
 * @param displayArgs Arguments passed in to gate.
 * @param cssClass Optional CSS class to apply to the unitary gate for styling.
 *
 * @returns SVG representation of unitary box.
 */
const _unitaryBox = (
  label: string,
  x: number,
  y: number,
  width: number,
  height: number = gateHeight,
  displayArgs?: string,
  cssClass?: string,
): SVGElement => {
  y -= gateHeight / 2;
  const uBox: SVGElement = box(x - width / 2, y, width, height);
  if (cssClass != null) {
    uBox.setAttribute("class", cssClass);
  }
  const labelY = y + height / 2 - (displayArgs == null ? 0 : 7);
  const labelText = text(label, x, labelY);
  _style_gate_text(labelText);

  const elems = [uBox, labelText];
  if (displayArgs != null) {
    const argStrY = y + height / 2 + 8;

    const argButton = text(displayArgs, x, argStrY, argsFontSize);
    _style_gate_text(argButton);
    argButton.setAttribute("class", "arg-button");
    elems.push(argButton);
  }

  return group(elems);
};

/**
 * Creates the SVG for a SWAP gate on y coords given by `renderData`.
 *
 * @param renderData - The render data containing information about the gate, including position and targets.
 * @param nestedDepth - The depth of nested operations (used for adjusting padding and positioning).
 *
 * @returns SVG representation of SWAP gate.
 */
const _swap = (renderData: GateRenderData, nestedDepth: number): SVGElement => {
  const { x, targetsY } = renderData;

  // Get SVGs of crosses
  const [x1, y1, x2, y2] = _gatePosition(renderData, nestedDepth);
  const ys = targetsY?.flatMap((y) => y as number[]) || [];

  const bg: SVGElement = box(x1, y1, x2, y2, "gate-swap");
  const crosses: SVGElement[] = ys.map((y) => _cross(x, y));
  const vertLine: SVGElement = line(x, ys[0], x, ys[1]);
  vertLine.style.pointerEvents = "none";
  return group([bg, ...crosses, vertLine]);
};

/**
 * Creates the SVG for an X gate
 *
 * @param renderData - The render data containing information about the gate, including position and targets.
 *
 * @returns SVG representation of X gate.
 */
const _x = (renderData: GateRenderData): SVGElement => {
  const { x, targetsY } = renderData;
  const ys = targetsY.flatMap((y) => y as number[]);
  return _oplus(x, ys[0]);
};

/**
 * Creates the SVG for a ket notation (e.g "|0⟩" or "|1⟩") gate.
 *
 * @param label    The label for the ket notation (e.g., "0" or "1").
 * @param renderData The render data containing information about the gate's position and appearance.
 *
 * @returns SVG representation of the ket notation gate.
 */
const _ket = (label: string, renderData: GateRenderData): SVGElement => {
  const { x, targetsY, width } = renderData;
  const gate = _unitary(
    `|${label}${mathChars.rangle}`,
    x,
    targetsY as number[][],
    width,
    undefined,
    false,
    "gate-ket",
  );
  gate.querySelector("text")!.classList.add("ket-text");
  return gate;
};

/**
 * Generates cross for display in SWAP gate.
 *
 * @param x x coord of gate.
 * @param y y coord of gate.
 *
 * @returns SVG representation for cross.
 */
const _cross = (x: number, y: number): SVGElement => {
  const radius = 8;
  const line1: SVGElement = line(
    x - radius,
    y - radius,
    x + radius,
    y + radius,
  );
  const line2: SVGElement = line(
    x - radius,
    y + radius,
    x + radius,
    y - radius,
  );
  return group([line1, line2]);
};

/**
 * Produces the SVG representation of a controlled gate on multiple qubits.
 *
 * @param renderData Render data of controlled gate.
 *
 * @returns SVG representation of controlled gate.
 */
const _controlledGate = (
  renderData: GateRenderData,
  nestedDepth: number,
): SVGElement => {
  const targetGateSvgs: SVGElement[] = [];
  const { type, x, controlsY, label, displayArgs, width } = renderData;
  let { targetsY } = renderData;

  // Get SVG for target gates
  switch (type) {
    case GateType.Cnot:
      (targetsY as number[]).forEach((y) => targetGateSvgs.push(_oplus(x, y)));
      break;
    case GateType.Swap:
      (targetsY as number[]).forEach((y) => targetGateSvgs.push(_cross(x, y)));
      break;
    case GateType.ControlledUnitary:
      {
        const groupedTargetsY: number[][] = targetsY as number[][];
        targetGateSvgs.push(
          _unitary(label, x, groupedTargetsY, width, displayArgs, false),
        );
        targetsY = targetsY.flat();
      }
      break;
    default:
      throw new Error(`ERROR: Unrecognized gate: ${label} of type ${type}`);
  }
  // Get SVGs for control dots
  const controlledDotsSvg: SVGElement[] = controlsY.map((y) =>
    controlDot(x, y),
  );
  // Create control lines
  const maxY: number = Math.max(...controlsY, ...(targetsY as number[]));
  const minY: number = Math.min(...controlsY, ...(targetsY as number[]));
  const vertLine: SVGElement = line(x, minY, x, maxY);
  vertLine.style.pointerEvents = "none";
  const svg: SVGElement = _createGate(
    [vertLine, ...controlledDotsSvg, ...targetGateSvgs],
    renderData,
    nestedDepth,
  );
  return svg;
};

/**
 * Generates $\oplus$ symbol for display in CNOT gate.
 *
 * @param x x coordinate of gate.
 * @param y y coordinate of gate.
 * @param r radius of circle.
 *
 * @returns SVG representation of $\oplus$ symbol.
 */
const _oplus = (x: number, y: number, r = 15): SVGElement => {
  const circleBorder: SVGElement = circle(x, y, r);
  const vertLine: SVGElement = line(x, y - r, x, y + r);
  const horLine: SVGElement = line(x - r, y, x + r, y);
  return group([circleBorder, vertLine, horLine], { class: "oplus" });
};

/**
 * Generates the SVG for a group of nested operations.
 *
 * @param renderData Render data of gate.
 * @param nestedDepth Depth of nested operations (used in classically controlled and grouped operations).
 *
 * @returns SVG representation of gate.
 */
const _groupedOperations = (
  renderData: GateRenderData,
  nestedDepth: number,
): SVGElement => {
  const { children } = renderData;
  const [x1, y1, x2, y2] = _gatePosition(renderData, nestedDepth);

  // Draw dashed box around children gates
  const box: SVGElement = dashedBox(x1, y1, x2, y2);
  const elems: SVGElement[] = [box];
  if (children != null)
    elems.push(formatGates(children as GateRenderData[][], nestedDepth + 1));
  return _createGate(elems, renderData, nestedDepth);
};

/**
 * Generates the SVG for a classically controlled group of operations.
 *
 * @param renderData Render data of gate.
 * @param padding  Padding within dashed box.
 *
 * @returns SVG representation of gate.
 */
const _classicalControlled = (
  renderData: GateRenderData,
  padding: number = groupBoxPadding,
): SVGElement => {
  const { controlsY, dataAttributes } = renderData;
  const targetsY: number[] = renderData.targetsY as number[];
  const children: GateRenderData[][][] =
    renderData.children as GateRenderData[][][];
  let { x, width } = renderData;

  const controlY = controlsY[0];

  const elems: SVGElement[] = [];

  if (children != null) {
    if (children.length !== 2)
      throw new Error(
        `Invalid number of children found for classically-controlled gate: ${children.length}`,
      );

    // Get SVG for gates controlled on 0
    const childrenZero: SVGElement = formatGates(children[0]);
    childrenZero.setAttribute("class", "gates-zero");
    elems.push(childrenZero);

    // Get SVG for gates controlled on 1
    const childrenOne: SVGElement = formatGates(children[1]);
    childrenOne.setAttribute("class", "gates-one");
    elems.push(childrenOne);
  }

  // Draw control button and attached dashed line to dashed box
  const controlCircleX: number = x + controlBtnRadius;
  const controlCircle: SVGElement = _controlCircle(controlCircleX, controlY);
  const lineY1: number = controlY + controlBtnRadius,
    lineY2: number = controlY + classicalRegHeight / 2;
  const vertLine: SVGElement = dashedLine(
    controlCircleX,
    lineY1,
    controlCircleX,
    lineY2,
    "classical-line",
  );
  x += controlBtnOffset;
  const horLine: SVGElement = dashedLine(
    controlCircleX,
    lineY2,
    x,
    lineY2,
    "classical-line",
  );

  width = width - controlBtnOffset + (padding - groupBoxPadding) * 2;
  x += groupBoxPadding - padding;
  const y: number = targetsY[0] - gateHeight / 2 - padding;
  const height: number = targetsY[1] - targetsY[0] + gateHeight + padding * 2;

  // Draw dashed box around children gates
  const box: SVGElement = dashedBox(x, y, width, height, "classical-container");

  elems.push(...[horLine, vertLine, controlCircle, box]);

  // Display controlled operation in initial "unknown" state
  const attributes: { [attr: string]: string } = {
    class: `classically-controlled-group classically-controlled-unknown`,
  };
  if (dataAttributes != null)
    Object.entries(dataAttributes).forEach(
      ([attr, val]) => (attributes[`data-${attr}`] = val),
    );

  return group(elems, attributes);
};

/**
 * Generates the SVG representation of the control circle on a classical register with interactivity support
 * for toggling between bit values (unknown, 1, and 0).
 *
 * @param x   x coord.
 * @param y   y coord.
 * @param r   Radius of circle.
 *
 * @returns SVG representation of control circle.
 */
const _controlCircle = (
  x: number,
  y: number,
  r: number = controlBtnRadius,
): SVGElement =>
  group([circle(x, y, r), text("?", x, y, labelFontSize)], {
    class: "classically-controlled-btn",
  });

export { formatGates, formatGate };
