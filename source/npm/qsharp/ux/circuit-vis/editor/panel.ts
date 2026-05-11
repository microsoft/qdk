// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import {
  Ket,
  Measurement,
  Operation,
  Unitary,
  type Circuit,
} from "../data/circuit.js";
import { ensureStateVisualization } from "../state-viz/stateVizController.js";
import type { StateColumn } from "../state-viz/stateViz.js";
import type { PrepareStateVizOptions } from "../state-viz/worker/stateVizPrep.js";
import {
  gateHeight,
  horizontalGap,
  minGateWidth,
  verticalGap,
} from "../renderer/constants.js";
import { formatGate } from "../renderer/formatters/gateFormatter.js";
import { GateType, GateRenderData } from "../renderer/gateRenderData.js";
import { getMinGateWidth } from "../utils.js";

const getOrCreateCircuitWrapper = (container: HTMLElement) => {
  let wrapper: HTMLElement | null = container.querySelector(".circuit-wrapper");
  const circuit = container.querySelector("svg.qviz") as SVGElement | null;
  if (circuit == null) {
    throw new Error("No circuit found in the container");
  }
  if (!wrapper) {
    wrapper = document.createElement("div");
    wrapper.className = "circuit-wrapper";
    wrapper.appendChild(circuit);
    container.appendChild(wrapper);
  } else if (circuit.parentElement !== wrapper) {
    wrapper.appendChild(circuit);
  }

  return { wrapper, circuit };
};

const attachToolboxPanelIfMissing = (
  container: HTMLElement,
  createToolboxPanel: () => HTMLElement,
): void => {
  if (container.querySelector(".panel") != null) return;
  container.prepend(createToolboxPanel());
};

const removeEmptyCircuitMessage = (wrapper: HTMLElement): void => {
  const prevMsg = wrapper.querySelector(".empty-circuit-message");
  if (prevMsg) prevMsg.remove();
};

const addEmptyCircuitMessageIfEmpty = (
  wrapper: HTMLElement,
  circuit: SVGElement,
): void => {
  const wiresGroup = circuit?.querySelector(".wires");
  if (!wiresGroup || wiresGroup.children.length === 0) {
    const emptyMsg = document.createElement("div");
    emptyMsg.className = "empty-circuit-message";
    emptyMsg.textContent =
      "Your circuit is empty. Drag gates from the toolbox to get started!";
    wrapper.appendChild(emptyMsg);
  }
};

const applyCircuitEditorLayoutClasses = (
  container: HTMLElement,
  wrapper: HTMLElement,
): void => {
  container.classList.add("circuit-editor-container");
  wrapper.classList.add("circuit-wrapper");
};

/**
 * Create a panel for the circuit visualization.
 *
 * The toolbox always renders. The Run button only renders when
 * `runCallback` is provided. Hosts that can't run circuits (e.g. a
 * read-only preview, or any embedding without execution support)
 * just omit the callback and no button is created.
 *
 * @param container         HTML element for rendering visualization into
 * @param computeStateVizColumnsForCircuitModel Optional callback to compute
 *   state visualization columns from a circuit model, which enables state
 *   visualization features when provided.
 * @param runCallback       Optional callback invoked when the user clicks
 *   the Run button. When omitted, no Run button is rendered.
 */
const createPanel = (
  container: HTMLElement,
  computeStateVizColumnsForCircuitModel?: (
    model: Circuit,
    opts?: PrepareStateVizOptions,
  ) => Promise<StateColumn[]>,
  runCallback?: () => void,
): void => {
  const { wrapper, circuit } = getOrCreateCircuitWrapper(container);
  removeEmptyCircuitMessage(wrapper);
  attachToolboxPanelIfMissing(container, () => _panel(runCallback));
  addEmptyCircuitMessageIfEmpty(wrapper, circuit);
  applyCircuitEditorLayoutClasses(container, wrapper);

  ensureStateVisualization(container, computeStateVizColumnsForCircuitModel);
};

/**
 * Function to produce panel element
 * @param runCallback   Optional Run-button click handler. When omitted,
 *                      no Run button is rendered.
 * @returns             HTML element for panel
 */
const _panel = (runCallback?: () => void): HTMLElement => {
  const panelElem = _elem("div");
  panelElem.className = "panel";
  _children(panelElem, [_createToolbox(runCallback)]);
  return panelElem;
};

/**
 * Function to produce toolbox element
 * @param runCallback   Optional Run-button click handler. When omitted,
 *                      no Run button is rendered.
 * @returns             HTML element for toolbox
 */
const _createToolbox = (runCallback?: () => void): HTMLElement => {
  // Generate gate elements in a 3xN grid
  let prefixX = 0;
  let prefixY = 0;
  const gateElems = Object.keys(toolboxGateDictionary).map((key, index) => {
    const { width: gateWidth } = toRenderData(toolboxGateDictionary[key], 0, 0);

    // Increment prefixX for every gate, and reset after 2 gates (2 columns)
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
 * Function to create the run button in the toolbox panel
 * @param buttonY      Y coordinate for the top of the button
 * @param onClick      Click handler. The button is created visible and
 *                     pre-wired. Callers only get this far if they
 *                     actually want a Run button.
 * @returns            SVG group element containing the run button
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

  // Add elements to group
  runButtonGroup.appendChild(rect);
  runButtonGroup.appendChild(text);

  runButtonGroup.addEventListener("click", onClick);
  return runButtonGroup;
};

/**
 * Factory function to produce HTML element
 * @param tag       Tag name
 * @param className Class name
 * @returns         HTML element
 */
const _elem = (tag: string, className?: string): HTMLElement => {
  const _elem = document.createElement(tag);
  if (className) {
    _elem.className = className;
  }
  return _elem;
};

/**
 * Append all child elements to a parent HTML element
 * @param parentElem    Parent HTML element
 * @param childElems    Array of HTML child elements
 * @returns             Parent HTML element with all children appended
 */
const _children = (
  parentElem: HTMLElement,
  childElems: HTMLElement[],
): HTMLElement => {
  childElems.map((elem) => parentElem.appendChild(elem));
  return parentElem;
};

/**
 * Append all child elements to a parent SVG element
 * @param parentElem    Parent SVG element
 * @param childElems    Array of SVG child elements
 * @returns             Parent SVG element with all children appended
 */
const _childrenSvg = (
  parentElem: SVGElement,
  childElems: SVGElement[],
): SVGElement => {
  childElems.map((elem) => parentElem.appendChild(elem));
  return parentElem;
};

/**
 * Function to produce title element
 * @param text  Text
 * @returns     Title element
 */
const _title = (text: string): HTMLElement => {
  const titleElem = _elem("h2");
  titleElem.className = "title";
  titleElem.textContent = text;
  return titleElem;
};

/**
 * Wrapper to generate render data based on _opToRenderData with mock registers and limited support
 * @param operation     Operation object
 * @param x             x coordinate at starting point from the left
 * @param y             y coordinate at starting point from the top
 * @returns             GateRenderData object
 */
const toRenderData = (
  operation: Operation | undefined,
  x: number,
  y: number,
): GateRenderData => {
  const target = y + 1 + gateHeight / 2; // offset by 1 for top padding
  const renderData: GateRenderData = {
    type: GateType.Invalid,
    isExpanded: false,
    x: x + 1 + minGateWidth / 2, // offset by 1 for left padding
    controlsY: [],
    targetsY: [target],
    label: "",
    width: -1,
    topPadding: 0,
    bottomPadding: 0,
  };

  if (operation === undefined) return renderData;

  switch (operation.kind) {
    case "unitary": {
      const { gate, controls } = operation;

      if (gate === "SWAP") {
        renderData.type = GateType.Swap;
      } else if (controls && controls.length > 0) {
        renderData.type =
          gate === "X" ? GateType.Cnot : GateType.ControlledUnitary;
        renderData.label = gate;
        if (gate !== "X") {
          renderData.targetsY = [[target]];
        }
      } else if (gate === "X") {
        renderData.type = GateType.X;
        renderData.label = gate;
      } else {
        renderData.type = GateType.Unitary;
        renderData.label = gate;
        renderData.targetsY = [[target]];
      }
      break;
    }
    case "measurement":
      renderData.type = GateType.Measure;
      renderData.controlsY = [target];
      break;
    case "ket":
      renderData.type = GateType.Ket;
      renderData.label = operation.gate;
      renderData.targetsY = [[target]];
      break;
  }

  if (operation.args !== undefined && operation.args.length > 0)
    renderData.displayArgs = operation.args[0];

  renderData.width = getMinGateWidth(renderData);
  renderData.x = x + 1 + renderData.width / 2; // offset by 1 for left padding

  return renderData;
};

/**
 * Generate an SVG gate element for the Toolbox panel based on the type of gate.
 * This function retrieves the operation render data from the gate dictionary,
 * formats the gate, and returns the corresponding SVG element.
 *
 * @param gateDictionary - The dictionary containing gate operations.
 * @param type - The type of gate. Example: 'H' or 'X'.
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

/**
 * Interface for gate dictionary
 */
interface GateDictionary {
  [index: string]: Operation;
}

/**
 * Function to create a unitary operation
 *
 * @param gate - The name of the gate
 * @returns Unitary operation object
 */
const _makeUnitary = (gate: string): Unitary => {
  return {
    kind: "unitary",
    gate: gate,
    targets: [],
  };
};

/**
 * Function to create a measurement operation
 *
 * @param gate - The name of the gate
 * @returns Unitary operation object
 */
const _makeMeasurement = (gate: string): Measurement => {
  return {
    kind: "measurement",
    gate: gate,
    qubits: [],
    results: [],
  };
};

const _makeKet = (gate: string): Ket => {
  return {
    kind: "ket",
    gate: gate,
    targets: [],
  };
};

/**
 * Object for default gate dictionary
 */
const toolboxGateDictionary: GateDictionary = {
  RX: _makeUnitary("Rx"),
  X: _makeUnitary("X"),
  RY: _makeUnitary("Ry"),
  Y: _makeUnitary("Y"),
  RZ: _makeUnitary("Rz"),
  Z: _makeUnitary("Z"),
  S: _makeUnitary("S"),
  T: _makeUnitary("T"),
  H: _makeUnitary("H"),
  SX: _makeUnitary("SX"),
  Reset: _makeKet("0"),
  Measure: _makeMeasurement("Measure"),
};

toolboxGateDictionary["RX"].params = [{ name: "theta", type: "Double" }];
toolboxGateDictionary["RY"].params = [{ name: "theta", type: "Double" }];
toolboxGateDictionary["RZ"].params = [{ name: "theta", type: "Double" }];

export { createPanel, toolboxGateDictionary, toRenderData };
