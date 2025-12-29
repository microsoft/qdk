// Snapshot wiring is disabled for now; we'll render static mock data.
// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { Ket, Measurement, Operation, Unitary } from "./circuit.js";
import {
  gateHeight,
  horizontalGap,
  minGateWidth,
  verticalGap,
} from "./constants.js";
import { formatGate } from "./formatters/gateFormatter.js";
import { GateType, GateRenderData } from "./gateRenderData.js";
import { getGateWidth } from "./utils.js";

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
    wrapper.style.display = "block";
    wrapper.style.overflow = "auto";
    wrapper.style.width = "100%";
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
    emptyMsg.style.padding = "2em";
    emptyMsg.style.textAlign = "center";
    emptyMsg.style.color = "#888";
    emptyMsg.style.fontSize = "1.1em";
    wrapper.appendChild(emptyMsg);
  }

  // Ensure flex layout
  container.style.display = "flex";
  container.style.height = "80vh";
  container.style.width = "95vw";
  container.style.alignItems = "stretch";
  wrapper.style.flex = "1 1 auto";
  wrapper.style.minWidth = "0";

  // Ensure a right-side state panel exists
  if (container.querySelector(".state-panel") == null) {
    const statePanel = createStatePanel();
    statePanel.style.position = "relative";
    statePanel.style.zIndex = "10";
    statePanel.style.pointerEvents = "auto";
    statePanel.style.flexShrink = "0";
    statePanel.style.flexGrow = "0";
    statePanel.style.flexBasis = "360px";
    container.appendChild(statePanel);
  }

  // Render static mock data in the state panel immediately.
  const panelElem = container.querySelector(
    ".state-panel",
  ) as HTMLElement | null;
  if (panelElem) {
    updateStatePanel(panelElem, getStaticMockSnapshot());
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
 * @returns             HTML element for panel
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

/**
 * Lightweight state panel (right-side) showing a bar chart of state probabilities
 * with phase encoded as hue. Designed to be minimally invasive to existing code.
 */
type SnapshotState = {
  nQubits: number;
  basis: string;
  amplitudes?: Array<{ index: number; ampRe: number; ampIm: number }>;
  topK?: Array<{
    index: number;
    prob: number;
    phase: number;
    ampRe: number;
    ampIm: number;
  }>;
  tailMass?: number;
  shots?: Array<{ bitstring: string; count: number }>;
  mode: "ideal" | "noisy";
  norm: number;
};

type RenderOptions = {
  maxBars?: number;
  heightPx?: number;
  widthPx?: number;
  phaseColorMap?: (phaseRad: number) => string;
};

const _defaultPhaseColor = (phi: number) => {
  const hue = ((phi + Math.PI) / (2 * Math.PI)) * 360;
  return `hsl(${hue},70%,50%)`;
};

// Format phase in multiples of π, e.g., -0.50π, +0.25π
const _formatPhasePi = (phi: number): string => {
  const k = phi / Math.PI;
  const sign = k >= 0 ? "+" : "";
  return `${sign}${k.toFixed(2)}π`;
};

// Static mock snapshot used for non-representative bar rendering.
const getStaticMockSnapshot = (): SnapshotState => {
  const nQubits = 3;
  const probs = [0.35, 0.2, 0.1, 0.08, 0.07, 0.06, 0.03, 0.01];
  const phaseCount = probs.length;
  const topK = probs.map((p, i) => ({
    index: i,
    prob: p,
    // Evenly span phases from -π to +π across bars
    phase: phaseCount > 1 ? -Math.PI + (2 * Math.PI * i) / (phaseCount - 1) : 0,
    ampRe: Math.sqrt(p),
    ampIm: 0,
  }));
  const tailMass = Math.max(0, 1 - probs.reduce((a, b) => a + b, 0));
  return {
    nQubits,
    basis: "computational",
    topK,
    tailMass,
    mode: "ideal",
    norm: 1.0,
  };
};

const createStatePanel = (): HTMLElement => {
  const panel = document.createElement("div");
  panel.className = "state-panel";

  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.classList.add("state-svg");
  svg.setAttribute("width", "100%");
  svg.setAttribute("height", "100%");
  // Styling moved to CSS (.state-panel .state-svg)

  panel.appendChild(svg);
  return panel;
};

const updateStatePanel = (
  panel: HTMLElement,
  snap: SnapshotState,
  opts: RenderOptions = {},
): void => {
  const svg = panel.querySelector("svg.state-svg") as SVGSVGElement | null;
  if (!svg) return;

  const width = svg.clientWidth || 340;
  const height = svg.clientHeight || 260;
  const margin = { top: 0, right: 10, bottom: 48, left: 28 };

  while (svg.firstChild) svg.removeChild(svg.firstChild);

  const barsData =
    snap.amplitudes && snap.nQubits <= 8
      ? snap.amplitudes.map((a) => {
          const prob = a.ampRe * a.ampRe + a.ampIm * a.ampIm;
          const phase = Math.atan2(a.ampIm, a.ampRe);
          return { index: a.index, prob, phase };
        })
      : (snap.topK ?? []);

  const n = barsData.length;
  const spacing = 2;
  // Temporarily estimate width before final margin to size phase circles
  const wTemp = width - margin.left - margin.right;
  const bw = Math.max(2, Math.floor(wTemp / Math.max(1, n)) - spacing);
  const rCol = Math.max(6, Math.floor(bw / 2) - 1);
  const extraForBits = n <= 16 ? 18 : 0;
  const barHeaderSpace = 28;
  const phaseHeaderSpace = 20;
  const stateHeaderSpace = 20;
  const barLabelSpace = 22; // gap(6) + label(10) + gap(6)
  const phaseLabelSpace = 30; // add a bit more bottom gap for phase labels
  margin.bottom = Math.max(
    48,
    phaseHeaderSpace +
      rCol * 2 +
      phaseLabelSpace +
      stateHeaderSpace +
      extraForBits +
      24,
  );

  // Height is set on the SVG after rendering; bar section uses a fixed height.

  const g = document.createElementNS("http://www.w3.org/2000/svg", "g");
  g.setAttribute("transform", `translate(${margin.left},${margin.top})`);
  svg.appendChild(g);
  const phaseColor = opts.phaseColorMap ?? _defaultPhaseColor;

  const maxProb = Math.max(
    1e-12,
    Math.max(...barsData.map((b) => b.prob ?? 0)),
  );
  const maxBarSectionHeight = 180;
  // Restore previous fixed bar section height for consistent visuals
  const hBars = maxBarSectionHeight;
  const scaleY = (p: number) => (p / maxProb) * hBars;

  const totalProb = barsData.reduce((s, b) => s + (b.prob ?? 0), 0) || 1;

  // Section labels and separators
  const mkLabel = (text: string, x: number, y: number) => {
    const t = document.createElementNS("http://www.w3.org/2000/svg", "text");
    t.setAttribute("x", `${x}`);
    t.setAttribute("y", `${y}`);
    t.setAttribute("class", "state-header");
    t.textContent = text;
    return t;
  };
  const mkSep = (y: number) => {
    const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
    line.setAttribute("x1", "0");
    line.setAttribute("y1", `${y}`);
    line.setAttribute("x2", `${wTemp}`);
    line.setAttribute("y2", `${y}`);
    line.setAttribute("class", "state-separator");
    return line;
  };

  // Header rows (bold, left-aligned); content starts below each header space
  // Bars section: top separator at section start, header label below it
  // Header labels aligned to group left; no extra X needed.
  const sepBarY = 0;
  // No separator line above the Probability Density section; only the header
  g.appendChild(mkLabel("Probability Density", -8, sepBarY + 9));
  // Phase section: separator at end of bars content (top of phase section)
  const sepPhaseY = barHeaderSpace + hBars + barLabelSpace;
  g.appendChild(mkSep(sepPhaseY));
  g.appendChild(mkLabel("Phase", -8, sepPhaseY + 9));
  // State section: separator at end of phase content (top of state section)
  // Place the phase-state separator just below the circles (include padding)
  const sepStateY = sepPhaseY + phaseHeaderSpace + 2 * rCol + phaseLabelSpace;
  g.appendChild(mkSep(sepStateY));
  g.appendChild(mkLabel("State", -8, sepStateY + 9));

  barsData.forEach((b, i) => {
    const x = i * (bw + spacing);
    const bar = document.createElementNS("http://www.w3.org/2000/svg", "rect");
    bar.setAttribute("x", `${x}`);
    bar.setAttribute("y", `${barHeaderSpace + (hBars - scaleY(b.prob))}`);
    bar.setAttribute("width", `${bw}`);
    bar.setAttribute("height", `${scaleY(b.prob)}`);
    bar.setAttribute("fill", phaseColor(b.phase));
    bar.setAttribute("class", "state-bar");
    // Tooltip with probability and phase
    const tip = document.createElementNS("http://www.w3.org/2000/svg", "title");
    const pctTip = (100 * (b.prob ?? 0)) / totalProb;
    tip.textContent = `${pctTip.toFixed(1)}% • φ=${_formatPhasePi(b.phase)}`;
    bar.appendChild(tip);
    g.appendChild(bar);

    // Numeric label showing percentage relative to displayed bars
    if (bw >= 4) {
      const pct = (100 * (b.prob ?? 0)) / totalProb;
      const label = document.createElementNS(
        "http://www.w3.org/2000/svg",
        "text",
      );
      label.setAttribute("x", `${x + bw / 2}`);
      // Place percentage label below the bars within the section
      const labelY = barHeaderSpace + hBars + 6;
      label.setAttribute("y", `${labelY}`);
      label.setAttribute("class", "state-bar-label");
      label.textContent =
        pct >= 1 ? `${pct.toFixed(0)}%` : `${pct.toFixed(1)}%`;
      g.appendChild(label);
    }

    // Phase indicator circle below each bar, sized to column width, with phase label inside
    const cx = x + bw / 2;
    const r = rCol;
    const phaseContentYBase = sepPhaseY + phaseHeaderSpace;
    const cy = phaseContentYBase + r + 8;
    // Angle wedge (sector) inside the phase circle from 0 to φ
    const sx = cx + r;
    const sy = cy;
    const ex = cx + r * Math.cos(b.phase);
    const ey = cy - r * Math.sin(b.phase);
    const largeArc = Math.abs(b.phase) > Math.PI ? 1 : 0;
    const sweep = b.phase < 0 ? 1 : 0; // CW for negative φ, CCW for positive φ
    const wedge = document.createElementNS(
      "http://www.w3.org/2000/svg",
      "path",
    );
    const d = `M ${cx} ${cy} L ${sx} ${sy} A ${r} ${r} 0 ${largeArc} ${sweep} ${ex} ${ey} Z`;
    wedge.setAttribute("d", d);
    wedge.setAttribute("class", "state-phase-wedge");
    wedge.setAttribute("fill", phaseColor(b.phase));
    g.appendChild(wedge);
    const circle = document.createElementNS(
      "http://www.w3.org/2000/svg",
      "circle",
    );
    circle.setAttribute("cx", `${cx}`);
    circle.setAttribute("cy", `${cy}`);
    circle.setAttribute("r", `${r}`);
    circle.setAttribute("class", "state-phase-circle");
    const tipPhase = document.createElementNS(
      "http://www.w3.org/2000/svg",
      "title",
    );
    tipPhase.textContent = `φ=${_formatPhasePi(b.phase)}`;
    circle.appendChild(tipPhase);
    g.appendChild(circle);

    // Phase text centered inside the circle
    const phaseText = document.createElementNS(
      "http://www.w3.org/2000/svg",
      "text",
    );
    phaseText.setAttribute("x", `${cx}`);
    // Place phase text below the circle
    phaseText.setAttribute("y", `${cy + r + 6}`);
    // Font size controlled by CSS (.state-phase-text)
    phaseText.setAttribute("class", "state-phase-text");
    phaseText.textContent = _formatPhasePi(b.phase);
    g.appendChild(phaseText);

    // Small dot on the circle perimeter indicating phase angle
    const dx = r * Math.cos(b.phase);
    const dy = r * Math.sin(b.phase);
    const dot = document.createElementNS(
      "http://www.w3.org/2000/svg",
      "circle",
    );
    dot.setAttribute("cx", `${cx + dx}`);
    dot.setAttribute("cy", `${cy - dy}`);
    dot.setAttribute("r", `${Math.max(1.5, r * 0.2)}`);
    dot.setAttribute("fill", phaseColor(b.phase));
    dot.setAttribute("class", "state-phase-dot");
    g.appendChild(dot);

    if (n <= 16) {
      const bit = b.index.toString(2).padStart(snap.nQubits, "0");
      const t = document.createElementNS("http://www.w3.org/2000/svg", "text");
      t.setAttribute("x", `${x + bw / 2}`);
      const stateContentYBase = sepStateY + stateHeaderSpace;
      t.setAttribute("y", `${stateContentYBase + 12}`);
      t.setAttribute("class", "state-bitstring");
      t.textContent = bit;
      g.appendChild(t);
    }
  });

  // Simplified: no axis, legend, or header — bars with percentage labels and optional bitstring labels

  // Size the SVG to the content so the panel can scroll if needed
  try {
    const bbox = g.getBBox();
    const svgHeight = Math.max(height, Math.ceil(bbox.height + margin.top + 8));
    svg.setAttribute("height", svgHeight.toString());
    svg.setAttribute("width", "100%");
  } catch {
    // Fallback: keep existing height if getBBox is unavailable
  }
};

export { createStatePanel, updateStatePanel };

// --- Snapshot API adapter (stub) ---

type SnapshotRequest = {
  circuitId?: string;
  columnIndex: number;
  maxTopK?: number;
  sampleShots?: number;
  mode?: "ideal" | "noisy";
  componentGrid?: any;
};

const getSnapshot = async (req: SnapshotRequest): Promise<SnapshotState> => {
  // TODO: replace with WASM-backed implementation. For now, produce demo data.
  const nQubits = 3;
  const maxTopK = req.maxTopK ?? 8;
  const phase = (i: number) => -Math.PI + (2 * Math.PI * (i % 100)) / 100;
  const amps = Array.from({ length: Math.min(1 << nQubits, 16) }, (_, i) => {
    const p = Math.exp(-((i - (req.columnIndex ?? 0)) ** 2) / 8);
    const norm = Math.sqrt(
      Array.from({ length: Math.min(1 << nQubits, 16) }, (_, j) =>
        Math.exp(-((j - (req.columnIndex ?? 0)) ** 2) / 8),
      ).reduce((a, b) => a + b, 0),
    );
    const mag = Math.sqrt(p) / (norm || 1);
    return {
      index: i,
      ampRe: mag * Math.cos(phase(i)),
      ampIm: mag * Math.sin(phase(i)),
    };
  });
  const topK = amps
    .map((a) => {
      const prob = a.ampRe * a.ampRe + a.ampIm * a.ampIm;
      const ph = Math.atan2(a.ampIm, a.ampRe);
      return {
        index: a.index,
        prob,
        phase: ph,
        ampRe: a.ampRe,
        ampIm: a.ampIm,
      };
    })
    .sort((x, y) => y.prob - x.prob)
    .slice(0, maxTopK);
  const tailMass = Math.max(0, 1 - topK.reduce((s, k) => s + k.prob, 0));
  return {
    nQubits,
    basis: "computational",
    amplitudes: nQubits <= 8 ? amps : undefined,
    topK,
    tailMass,
    mode: req.mode ?? "ideal",
    norm: 1.0,
  };
};

export { getSnapshot };
