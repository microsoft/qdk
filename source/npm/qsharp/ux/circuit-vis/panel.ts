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
    // Prefer the simplified map-based input for the visualizer
    updateStatePanelFromMap(panelElem, getStaticMockAmpMap(), {
      normalize: false,
    });
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
// SnapshotState removed: visualizer now consumes AmpMap directly.

type RenderOptions = {
  maxBars?: number;
  heightPx?: number;
  widthPx?: number;
  phaseColorMap?: (phaseRad: number) => string;
  normalize?: boolean; // normalize probabilities to unit mass (default true)
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

// Simplified input: map of state name (bitstring) -> amplitude.
// Accept either complex form `{ re, im }` or polar form `{ prob, phase }`.
type AmpComplex = { re: number; im: number };
type AmpPolar = { prob?: number; phase?: number };
type AmpLike = AmpComplex | AmpPolar;
type AmpMap = Record<string, AmpLike>;

// Convert amplitude to polar `{ prob, phase }`. If complex, compute; if polar, use directly.
const _toPolar = (a: AmpLike): { prob: number; phase: number } => {
  const maybe = a as Partial<AmpComplex & AmpPolar>;
  const hasPolar =
    typeof maybe.prob === "number" || typeof maybe.phase === "number";
  if (hasPolar) {
    const prob = typeof maybe.prob === "number" ? maybe.prob : 0;
    const phase = typeof maybe.phase === "number" ? maybe.phase : 0;
    return { prob, phase };
  }
  const re = typeof maybe.re === "number" ? maybe.re : 0;
  const im = typeof maybe.im === "number" ? maybe.im : 0;
  return { prob: re * re + im * im, phase: Math.atan2(im, re) };
};

// Helper: create an AmpMap from polar entries for easy mock building
const toAmpMapPolar = (
  items: Array<{ bit: string; prob?: number; phase?: number }>,
): AmpMap => {
  const m: AmpMap = {};
  for (const it of items) {
    m[it.bit] = { prob: it.prob ?? 0, phase: it.phase ?? 0 };
  }
  return m;
};

// Static mock map with a few non-zero amplitudes; other states are implicitly zero.
const getStaticMockAmpMap = (): AmpMap => {
  // 3-qubit example with evenly varied phases across states
  const states: Array<{ bit: string; p: number }> = [
    { bit: "000", p: 0.35 },
    { bit: "001", p: 0.2 },
    { bit: "010", p: 0.1 },
    { bit: "011", p: 0.0825 },
    { bit: "100", p: 0.07 },
    { bit: "101", p: 0.06 },
    { bit: "110", p: 0.03 },
    { bit: "111", p: 0.01 },
  ];
  const N = states.length;
  const ampMap: AmpMap = {};
  states.forEach((s, i) => {
    const phi = N > 1 ? -Math.PI + (2 * Math.PI * i) / (N - 1) : 0;
    const mag = Math.sqrt(s.p);
    ampMap[s.bit] = { re: mag * Math.cos(phi), im: mag * Math.sin(phi) };
  });

  delete ampMap["000"];
  delete ampMap["001"];
  delete ampMap["010"];
  // delete ampMap["011"];
  delete ampMap["100"];
  // delete ampMap["101"];
  delete ampMap["110"];
  delete ampMap["111"];

  return ampMap;
};

// Adapter: render from a map of named states to complex amplitudes.
// Handles missing states naturally; only provided keys are shown.
const updateStatePanelFromMap = (
  panel: HTMLElement,
  ampMap: AmpMap,
  opts: RenderOptions & { nQubits?: number } = {},
): void => {
  const entries = Object.entries(ampMap);
  if (entries.length === 0) {
    // Nothing to render; clear SVG
    const svg = panel.querySelector("svg.state-svg") as SVGSVGElement | null;
    if (svg) while (svg.firstChild) svg.removeChild(svg.firstChild);
    return;
  }

  const guessN =
    opts.nQubits ?? entries.reduce((m, [k]) => Math.max(m, k.length), 0);
  // Compute raw probabilities and phases
  const raw = entries.map(([bit, a]) => {
    const { prob, phase } = _toPolar(a);
    return { bit, prob, phase };
  });
  // Optional normalization to unit mass
  const doNormalize = opts.normalize ?? true;
  const mass = raw.reduce((s, r) => s + r.prob, 0);
  const states =
    doNormalize && mass > 0
      ? raw.map((r) => ({ ...r, prob: r.prob / mass }))
      : raw;

  // Ordering: numeric labels first (ascending), then non-numeric alphabetically
  const numericRegex = /^[+-]?\d+(?:\.\d+)?$/;
  const asNumber = (s: string) => (numericRegex.test(s) ? parseFloat(s) : NaN);
  const isNumeric = (s: string) => numericRegex.test(s);
  const labelCmp = (a: string, b: string) => a.localeCompare(b);

  // Sort by probability descending and optionally cap to maxBars
  const maxBars = opts.maxBars ?? 16;
  const sorted = states.sort((a, b) => {
    const an = isNumeric(a.bit);
    const bn = isNumeric(b.bit);
    if (an && bn) {
      const av = asNumber(a.bit);
      const bv = asNumber(b.bit);
      return av - bv;
    }
    if (an !== bn) return an ? -1 : 1;
    return labelCmp(a.bit, b.bit);
  });
  const top = sorted.slice(0, maxBars);
  renderStatePanelBars(panel, top, { ...opts, nQubits: guessN });
};

// Render helper that draws the state panel directly from bar data
type BarDatum = { bit: string; prob: number; phase: number };

const renderStatePanelBars = (
  panel: HTMLElement,
  barsData: BarDatum[],
  opts: RenderOptions & { nQubits?: number } = {},
): void => {
  const svg = panel.querySelector("svg.state-svg") as SVGSVGElement | null;
  if (!svg) return;

  const width = svg.clientWidth || opts.widthPx || 340;
  const height = svg.clientHeight || opts.heightPx || 260;
  const margin = { top: 0, right: 10, bottom: 48, left: 28 };

  while (svg.firstChild) svg.removeChild(svg.firstChild);

  const n = barsData.length;
  const spacing = 2;
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

  const g = document.createElementNS("http://www.w3.org/2000/svg", "g");
  g.setAttribute("transform", `translate(${margin.left},${margin.top})`);
  svg.appendChild(g);
  const phaseColor = opts.phaseColorMap ?? _defaultPhaseColor;

  const maxProb = Math.max(
    1e-12,
    Math.max(...barsData.map((b) => b.prob ?? 0)),
  );
  const hBars = 180; // fixed section height
  // Always scale relative to tallest bar so the max reaches full height.
  const scaleY = (p: number) => (p / maxProb) * hBars;

  // Labels and separators
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

  const sepBarY = 0;
  g.appendChild(mkLabel("Probability Density", -8, sepBarY + 9));
  const sepPhaseY = barHeaderSpace + hBars + barLabelSpace;
  g.appendChild(mkSep(sepPhaseY));
  g.appendChild(mkLabel("Phase", -8, sepPhaseY + 9));
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
    const tip = document.createElementNS("http://www.w3.org/2000/svg", "title");
    const pctTip = (b.prob ?? 0) * 100;
    tip.textContent = `${pctTip.toFixed(1)}% • φ=${_formatPhasePi(b.phase)}`;
    bar.appendChild(tip);
    g.appendChild(bar);

    if (bw >= 4) {
      const label = document.createElementNS(
        "http://www.w3.org/2000/svg",
        "text",
      );
      label.setAttribute("x", `${x + bw / 2}`);
      const labelY = barHeaderSpace + hBars + 6;
      label.setAttribute("y", `${labelY}`);
      label.setAttribute("class", "state-bar-label");
      const pct = (b.prob ?? 0) * 100;
      label.textContent =
        pct >= 1 ? `${pct.toFixed(0)}%` : `${pct.toFixed(1)}%`;
      g.appendChild(label);
    }

    const cx = x + bw / 2;
    const r = rCol;
    const phaseContentYBase = sepPhaseY + phaseHeaderSpace;
    const cy = phaseContentYBase + r + 8;
    const sx = cx + r;
    const sy = cy;
    const ex = cx + r * Math.cos(b.phase);
    const ey = cy - r * Math.sin(b.phase);
    const largeArc = Math.abs(b.phase) > Math.PI ? 1 : 0;
    const sweep = b.phase < 0 ? 1 : 0;
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

    const phaseText = document.createElementNS(
      "http://www.w3.org/2000/svg",
      "text",
    );
    phaseText.setAttribute("x", `${cx}`);
    phaseText.setAttribute("y", `${cy + r + 6}`);
    phaseText.setAttribute("class", "state-phase-text");
    phaseText.textContent = _formatPhasePi(b.phase);
    g.appendChild(phaseText);

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

    // Use provided bitstring for display if reasonable count
    if (n <= 16 && /^([01]+)$/.test(b.bit)) {
      const t = document.createElementNS("http://www.w3.org/2000/svg", "text");
      t.setAttribute("x", `${x + bw / 2}`);
      const stateContentYBase = sepStateY + stateHeaderSpace;
      t.setAttribute("y", `${stateContentYBase + 12}`);
      t.setAttribute("class", "state-bitstring");
      t.textContent = b.bit;
      g.appendChild(t);
    }
  });

  try {
    const bbox = g.getBBox();
    const svgHeight = Math.max(height, Math.ceil(bbox.height + margin.top + 8));
    svg.setAttribute("height", svgHeight.toString());
    svg.setAttribute("width", "100%");
  } catch {
    // Fallback: keep existing height if getBBox is unavailable
  }
};

const createStatePanel = (): HTMLElement => {
  const panel = document.createElement("div");
  panel.className = "state-panel";

  // Full-height clickable edge with vertical text
  const edge = document.createElement("div");
  edge.className = "state-edge";
  edge.setAttribute("role", "button");
  edge.setAttribute("tabindex", "0");
  edge.setAttribute("aria-label", "Toggle state panel");
  edge.setAttribute("aria-expanded", "true");
  const edgeText = document.createElement("span");
  edgeText.className = "state-edge-text";
  edgeText.textContent = "State Vizualization";
  edge.appendChild(edgeText);

  // Add top and bottom right-pointing triangle icons
  const mkEdgeIcon = (cls: string): SVGSVGElement => {
    const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.setAttribute("viewBox", "0 0 14 14");
    svg.setAttribute("aria-hidden", "true");
    svg.classList.add("edge-icon", cls);
    const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
    // Equilateral right-pointing triangle: base at x=4.25, y=3 and y=11; tip at x=11, y=7
    // Derived from equilateral geometry: (tip.x - base.x) = sqrt(3) * (base half-height)
    path.setAttribute("d", "M 4.25 11 L 11 7 L 4.25 3 Z");
    svg.appendChild(path);
    return svg as SVGSVGElement;
  };

  const iconTop = mkEdgeIcon("edge-icon-top");
  const iconBottom = mkEdgeIcon("edge-icon-bottom");
  edge.appendChild(iconTop);
  edge.appendChild(iconBottom);

  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.classList.add("state-svg");
  svg.setAttribute("width", "100%");
  svg.setAttribute("height", "100%");
  // Styling moved to CSS (.state-panel .state-svg)

  panel.appendChild(edge);
  panel.appendChild(svg);

  // Click/keyboard handlers: accordion-collapse to the right
  const toggleCollapsed = () => {
    const collapsed = panel.classList.toggle("collapsed");
    edge.setAttribute("aria-expanded", (!collapsed).toString());
  };
  edge.addEventListener("click", toggleCollapsed);
  edge.addEventListener("keydown", (ev) => {
    if (ev.key === "Enter" || ev.key === " ") {
      ev.preventDefault();
      toggleCollapsed();
    }
  });
  return panel;
};

// Snapshot-based update removed. Use renderStatePanelBars or updateStatePanelFromMap.
export { createStatePanel };
export { updateStatePanelFromMap, getStaticMockAmpMap, toAmpMapPolar };

// --- Snapshot API adapter (stub) ---

// Snapshot API stub removed.
