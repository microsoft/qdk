// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { Ket, Measurement, Operation, Unitary } from "./circuit.js";
import {
  createStatePanel,
  updateStatePanelFromMap,
  renderDefaultStatePanel,
  getStaticMockAmpMap,
} from "./stateViz.js";
import { computeAmpMapFromCurrentModel, Endianness } from "./stateCompute.js";
import {
  gateHeight,
  horizontalGap,
  minGateWidth,
  verticalGap,
} from "./constants.js";
import { formatGate } from "./formatters/gateFormatter.js";
import { GateType, GateRenderData } from "./gateRenderData.js";
import { getGateWidth } from "./utils.js";

let vizEndianness: Endianness = "big";
type DataMode = "live" | "mock";
let vizDataMode: DataMode = "live";
let vizMockSet = 0;
let vizMinProbThreshold = 0.001; // 0..1 default 0.1%
// Toggle to show/hide the developer toolbar for the state panel
const SHOW_STATE_DEV_TOOLBAR = true;

/**
 * Create a panel for the circuit visualization.
 * @param container     HTML element for rendering visualization into
 */
const createPanel = (container: HTMLElement): void => {
  // Find or create the wrapper
  let wrapper: HTMLElement | null = container.querySelector(".circuit-wrapper");
  const circuit = container.querySelector("svg.qviz");
  if (circuit == null) {
    throw new Error("No circuit found in the container");
  }
  if (!wrapper) {
    wrapper = _elem("div", "");
    wrapper.className = "circuit-wrapper";
    wrapper.appendChild(circuit);
    container.appendChild(wrapper);
  } else if (circuit.parentElement !== wrapper) {
    // If wrapper exists but SVG is not inside, ensure it's appended
    wrapper.appendChild(circuit);
  }

  // Remove any previous message
  const prevMsg = wrapper.querySelector(".empty-circuit-message");
  if (prevMsg) prevMsg.remove();

  // Ensure the toolbox panel exists on the left
  if (container.querySelector(".panel") == null) {
    const panelElem = _panel();
    container.prepend(panelElem);
  }

  // Check if the circuit is empty by inspecting the .wires group
  const wiresGroup = circuit?.querySelector(".wires");
  if (!wiresGroup || wiresGroup.children.length === 0) {
    const emptyMsg = document.createElement("div");
    emptyMsg.className = "empty-circuit-message";
    emptyMsg.textContent =
      "Your circuit is empty. Drag gates from the toolbox to get started!";
    wrapper.appendChild(emptyMsg);
  }

  // Ensure flex layout via CSS classes
  container.classList.add("circuit-editor-container");
  wrapper.classList.add("circuit-wrapper");

  // Ensure a right-side state panel exists
  if (container.querySelector(".state-panel") == null) {
    const statePanel = createStatePanel();
    container.appendChild(statePanel);
  }

  // Render static mock data in the state panel immediately.
  const panelElem = container.querySelector(
    ".state-panel",
  ) as HTMLElement | null;
  if (panelElem) {
    // Add a simple toolbar just above the SVG
    const svgElem = panelElem.querySelector("svg.state-svg");
    if (
      SHOW_STATE_DEV_TOOLBAR &&
      svgElem &&
      !panelElem.querySelector(".dev-toolbar")
    ) {
      const toolbar = document.createElement("div");
      toolbar.className = "dev-toolbar";

      // Endianness control
      const labelEndian = document.createElement("span");
      labelEndian.textContent = "Endianness:";
      const selEndian = document.createElement("select");
      selEndian.className = "endianness-select";
      const optBig = document.createElement("option");
      optBig.value = "big";
      optBig.text = "Big";
      const optLittle = document.createElement("option");
      optLittle.value = "little";
      optLittle.text = "Little";
      selEndian.appendChild(optBig);
      selEndian.appendChild(optLittle);
      selEndian.value = vizEndianness;
      selEndian.addEventListener("change", () => {
        vizEndianness = (selEndian.value as Endianness) ?? "big";
        renderState(panelElem);
      });
      toolbar.appendChild(labelEndian);
      toolbar.appendChild(selEndian);

      // Separator
      const sep = document.createElement("span");
      sep.className = "dev-toolbar-sep";
      sep.textContent = "|";
      toolbar.appendChild(sep);

      // Data mode control
      const labelMode = document.createElement("span");
      labelMode.textContent = "Data:";
      const selMode = document.createElement("select");
      selMode.className = "data-mode-select";
      const optLive = document.createElement("option");
      optLive.value = "live";
      optLive.text = "Live";
      const optMock = document.createElement("option");
      optMock.value = "mock";
      optMock.text = "Mock";
      selMode.appendChild(optLive);
      selMode.appendChild(optMock);
      selMode.value = vizDataMode;
      toolbar.appendChild(labelMode);
      toolbar.appendChild(selMode);

      // Mock set selector
      const labelMock = document.createElement("span");
      labelMock.textContent = "Mock set:";
      const selMock = document.createElement("select");
      selMock.className = "mock-set-select";
      for (let i = 0; i < 4; i++) {
        const opt = document.createElement("option");
        opt.value = String(i);
        opt.text = `#${i + 1}`;
        selMock.appendChild(opt);
      }
      selMock.value = String(vizMockSet);
      const applyMockVisibility = () => {
        const show = selMode.value === "mock";
        labelMock.style.display = show ? "" : "none";
        selMock.style.display = show ? "" : "none";
        // Endianness is irrelevant to mock data, but leave enabled to avoid UI jump
      };
      selMode.addEventListener("change", () => {
        vizDataMode = (selMode.value as DataMode) ?? "live";
        applyMockVisibility();
        renderState(panelElem);
      });
      selMock.addEventListener("change", () => {
        vizMockSet = parseInt(selMock.value) || 0;
        renderState(panelElem);
      });
      applyMockVisibility();
      toolbar.appendChild(labelMock);
      toolbar.appendChild(selMock);

      // Separator
      const sep2 = document.createElement("span");
      sep2.className = "dev-toolbar-sep";
      sep2.textContent = "|";
      toolbar.appendChild(sep2);

      // Minimum probability threshold control (percentage)
      const labelThresh = document.createElement("span");
      labelThresh.textContent = "Min %:";
      const inputThresh = document.createElement("input");
      inputThresh.type = "number";
      inputThresh.min = "0";
      inputThresh.max = "100";
      inputThresh.step = "0.1";
      inputThresh.value = "0.1";
      inputThresh.title =
        "States below this percentage are aggregated into Others";
      inputThresh.addEventListener("change", () => {
        const v = parseFloat(inputThresh.value);
        const pct = isFinite(v) && v > 0 ? Math.min(100, Math.max(0, v)) : 0;
        inputThresh.value = String(pct);
        vizMinProbThreshold = pct / 100;
        renderState(panelElem);
      });
      toolbar.appendChild(labelThresh);
      toolbar.appendChild(inputThresh);

      panelElem.insertBefore(toolbar, svgElem);
    }

    const renderState = (panel: HTMLElement) => {
      if (vizDataMode === "mock") {
        const ampMap = getStaticMockAmpMap(vizMockSet);
        updateStatePanelFromMap(panel, ampMap, {
          normalize: false,
          minProbThreshold: vizMinProbThreshold,
        });
        return true;
      }
      const ampMap = computeAmpMapFromCurrentModel(vizEndianness);
      if (ampMap) {
        updateStatePanelFromMap(panel, ampMap, {
          normalize: true,
          minProbThreshold: vizMinProbThreshold,
        });
        return true;
      } else {
        // Determine current wire count from the circuit DOM
        const circuit = container.querySelector("svg.qviz");
        const wiresGroup = circuit?.querySelector(".wires");
        const wireCount = wiresGroup ? wiresGroup.children.length : 0;
        renderDefaultStatePanel(panel, wireCount);
        return false;
      }
    };

    // Initial render; if the circuit model isn't ready yet, retry briefly until available
    const gotReal = renderState(panelElem);
    if (!gotReal) {
      let attempts = 20; // try for ~2 seconds total
      const retry = () => {
        if (renderState(panelElem)) return; // success swaps mock for real
        if (--attempts > 0) setTimeout(retry, 100);
      };
      setTimeout(retry, 100);
    }
  }
};

/**
 * Enable the run button in the toolbox panel.
 * This function makes the run button visible and adds a click event listener.
 * @param container     HTML element containing the toolbox panel
 * @param callback      Callback function to execute when the run button is clicked
 */
const enableRunButton = (
  container: HTMLElement,
  callback: () => void,
): void => {
  const runButton = container.querySelector(".svg-run-button");
  if (runButton && runButton.getAttribute("visibility") !== "visible") {
    runButton.setAttribute("visibility", "visible");
    runButton.addEventListener("click", callback);
  }
};

/**
 * Function to produce panel element
 */
const _panel = (): HTMLElement => {
  const panelElem = _elem("div");
  panelElem.className = "panel";
  _children(panelElem, [_createToolbox()]);
  return panelElem;
};

/**
 * Function to produce toolbox element
 * @returns             HTML element for toolbox
 */
const _createToolbox = (): HTMLElement => {
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

  // Append run button
  const runButtonGroup = _createRunButton(prefixY + gateHeight + 20);
  svgElem.appendChild(runButtonGroup);

  // Size SVG to content height so the toolbox panel can scroll when window is short
  const totalSvgHeight = prefixY + 2 * gateHeight + 32; // gates + button + padding
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
 * @returns            SVG group element containing the run button
 */
const _createRunButton = (buttonY: number): SVGGElement => {
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

  // The run button should be hidden by default
  runButtonGroup.setAttribute("visibility", "hidden");
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
    x: x + 1 + minGateWidth / 2, // offset by 1 for left padding
    controlsY: [],
    targetsY: [target],
    label: "",
    width: -1,
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

  renderData.width = getGateWidth(renderData);
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

export { createPanel, enableRunButton, toolboxGateDictionary, toRenderData };
