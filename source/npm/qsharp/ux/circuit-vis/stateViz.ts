// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// State visualization renderer.
// Defines the renderable column/types and updates the `.state-panel` DOM
// (SVG/HTML) to display probabilities/phases, given either an amp map or
// pre-prepared columns.

export type RenderOptions = {
  maxColumns?: number;
  phaseColorMap?: (phaseRad: number) => string;
  // Fill color for the aggregated "Others" column.
  // Default comes from VIZ.defaultOthersColor.
  othersColor?: string;
  normalize?: boolean; // normalize probabilities to unit mass (default true)
  minColumnWidth?: number; // minimum width per column to avoid label collisions
  minPanelWidthPx?: number; // prescribed minimum panel width in pixels
  maxPanelWidthPx?: number; // prescribed maximum panel width in pixels
  animationMs?: number; // global animation duration in ms (default 200ms)
  // Minimum probability (0..1) for a state to be shown as its own column.
  // States below this threshold will be aggregated into the Others bucket.
  // Default: 0 (thresholding off)
  minProbThreshold?: number;
};

import { prepareStateVizColumnsFromAmpMap } from "./stateVizPrep.js";

// Visualization config constants
const VIZ = {
  defaultAnimationMs: 200,
  baseHeight: 80,
  marginLeft: 36,
  marginRight: 36,
  marginBottom: 62,
  minPanelWidthPx: 80,
  defaultMinPanelWidth: 190,
  defaultOthersColor: "#a6a6a6",
  columnSpacing: 4,
  minColumnWidthFloor: 16,
  defaultMinColumnWidth: 36,
  defaultMaxColumns: 16,
  labelLongThresholdChars: 4,
  barHeaderPadding: 36,
  phaseHeaderPadding: 26,
  stateHeaderPadding: 26,
  percentLabelPadding: 29,
  phaseLabelPadding: 39,
  marginBottomMinPx: 48,
  extraBottomPaddingPx: 24,
  barAreaHeight: 234,
  minProbEpsilon: 1e-12,
  headerLabelXOffset: -8,
  headerLabelYOffset: 9,
  percentLabelOffset: 8,
  phaseRadiusBase: 12,
  phaseRadiusThreshold: 7.5,
  phaseCirclePaddingX: 2,
  phaseCirclePaddingY: 10,
  phaseDotFrac: 0.25,
  phaseDotRadiusMinPx: 1.5,
  phaseTextBottomPad: 6,
  verticalLabelCharHeight: 14,
  phaseLabelLineHeight: 14,
  verticalLabelExtraBase: 12,
  stateLabelVerticalOffset: 4,
  stateLabelHorizontalOffset: 16,
  contentHeightExtra: 10,
  edgePad: 36,
  emptyStateFlexBasisPx: 360,
  rowLabelFallbackPx: 24,
};

// --- Entry Points ---

export const createStatePanel = (initiallyExpanded = false): HTMLElement => {
  try {
    // Allows host environments (e.g., VS Code webview) to react to panel creation
    // without needing a direct import hook.
    (globalThis as any).dispatchEvent?.(
      new CustomEvent("qsharp:stateviz:create"),
    );
  } catch {
    // Ignore environments without CustomEvent / dispatchEvent
  }
  const panel = document.createElement("div");
  panel.className = "state-panel";
  if (!initiallyExpanded) {
    panel.classList.add("collapsed");
  }

  const edge = document.createElement("div");
  edge.className = "state-edge";
  edge.setAttribute("role", "button");
  edge.setAttribute("tabindex", "0");
  edge.setAttribute("aria-label", "Toggle state panel");
  edge.setAttribute("aria-expanded", initiallyExpanded.toString());
  const edgeText = document.createElement("span");
  edgeText.className = "state-edge-text";
  edgeText.textContent = "State Visualization";
  edge.appendChild(edgeText);

  const mkEdgeIcon = (cls: string): SVGSVGElement => {
    const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.setAttribute("viewBox", "0 0 14 14");
    svg.setAttribute("aria-hidden", "true");
    svg.classList.add("edge-icon", cls);
    const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
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

  panel.appendChild(edge);
  panel.appendChild(svg);

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

const ensureLoadingOverlay = (panel: HTMLElement): HTMLElement => {
  let overlay = panel.querySelector(
    ".state-loading-overlay",
  ) as HTMLElement | null;
  if (overlay) return overlay;

  overlay = document.createElement("div");
  overlay.className = "state-loading-overlay";
  overlay.setAttribute("aria-hidden", "true");

  const spinner = document.createElement("div");
  spinner.className = "state-loading-spinner";
  overlay.appendChild(spinner);

  panel.appendChild(overlay);
  return overlay;
};

export const setStatePanelLoading = (panel: HTMLElement, loading: boolean) => {
  if (!panel) return;
  if (loading) {
    ensureLoadingOverlay(panel);
    panel.classList.add("loading");
  } else {
    panel.classList.remove("loading");
  }
};

// Render a default state in the visualization panel.
// - If nQubits <= 0: hide the SVG and show a friendly message.
// - If nQubits > 0: show the SVG and render a zero-phase |0…0⟩ state immediately.
export const renderDefaultStatePanel = (
  panel: HTMLElement,
  nQubits: number,
): void => {
  const svg = panel.querySelector("svg.state-svg") as SVGSVGElement | null;
  if (!svg) return;

  if (!nQubits || nQubits <= 0) {
    // Hide SVG graphics and show message
    // Reset any stale height to avoid carrying over large values
    showEmptyState(panel);
  } else {
    // Remove message and render the deterministic zero-state
    showContentState(panel);
    const zeros = "0".repeat(nQubits);
    updateStatePanelFromMap(panel, { [zeros]: { re: 1, im: 0 } });
  }
};

export const renderUnsupportedStatePanel = (
  panel: HTMLElement,
  message: string,
): void => {
  showEmptyState(panel, message);
};

// Adapter: render from a map of named states to complex amplitudes.
export const updateStatePanelFromMap = (
  panel: HTMLElement,
  ampMap: AmpMap,
  opts: RenderOptions = {},
): void => {
  const columns = prepareStateVizColumnsFromAmpMap(ampMap, {
    normalize: opts.normalize,
    minProbThreshold: opts.minProbThreshold,
    maxColumns: opts.maxColumns,
  });
  updateStatePanelFromColumns(panel, columns, opts);
};

// Render from precomputed columns (e.g., prepared in a Web Worker).
export const updateStatePanelFromColumns = (
  panel: HTMLElement,
  columns: StateColumn[],
  opts: RenderOptions = {},
): void => {
  if (!columns || columns.length === 0) {
    showEmptyState(panel);
    return;
  }
  showContentState(panel);
  renderStatePanel(panel, columns, opts);
};

const showEmptyState = (
  panel: HTMLElement,
  message = "The circuit is empty.",
): void => {
  const svg = panel.querySelector("svg.state-svg") as SVGSVGElement | null;
  if (svg) {
    while (svg.firstChild) svg.removeChild(svg.firstChild);
    svg.removeAttribute("height");
  }
  panel.classList.add("empty");

  let msg = panel.querySelector(".state-empty-message") as HTMLElement | null;
  if (!msg) {
    msg = document.createElement("div");
    msg.className = "state-empty-message";
    panel.appendChild(msg);
  }
  msg.textContent = message;
  panel.style.flexBasis = `${VIZ.emptyStateFlexBasisPx}px`;
};

const showContentState = (panel: HTMLElement): void => {
  // Content visibility restored via CSS when not in empty mode
  // Remove empty mode to reveal content via CSS
  panel.classList.remove("empty");
  const emptyMsg = panel.querySelector(".state-empty-message");
  if (emptyMsg) emptyMsg.remove();
};

// --- State Amplitude Preparation ---

export type AmpComplex = { re: number; im: number };
export type AmpPolar = { prob?: number; phase?: number };
export type AmpLike = AmpComplex | AmpPolar;
export type AmpMap = Record<string, AmpLike>;

// --- Layout Computation ---

// The data for a single column in the state visualization
export type StateColumn = {
  label: string;
  prob: number;
  phase: number;
  isOthers?: boolean;
  othersCount?: number;
};

// Layout metrics used to organize rendering logic
type LayoutMetrics = {
  panelWidthPx: number;
  contentWidthPx: number;

  columnWidthPx: number;

  maxProb: number;

  phaseSectionTopY: number;
  phaseCircleRadiusPx: number;

  stateSectionTopY: number;
  verticalLabels: boolean;
  maxLabelLen: number;

  animationMs: number;
  othersColor: string;
  phaseColor: (phi: number) => string;
};

// The animation speed for the state viz panel can be set via the passed in options
// argument, or via CSS custom property `--stateAnimMs` on the panel element.
// This function computes the effective animation duration in milliseconds from
// these sources, with the CSS value taking precedence over the options argument.
const getAnimationMs = (panel: HTMLElement, opts: RenderOptions): number => {
  let animationMs = VIZ.defaultAnimationMs;
  if (
    typeof opts.animationMs === "number" &&
    isFinite(opts.animationMs) &&
    opts.animationMs >= 0
  ) {
    animationMs = Math.round(opts.animationMs);
  }
  if (panel.isConnected) {
    const cssDur = getComputedStyle(panel)
      .getPropertyValue("--stateAnimMs")
      .trim();
    const parsed = parseDurationMs(cssDur);
    if (!isNaN(parsed) && parsed >= 0) animationMs = parsed;
  }
  return animationMs;
};

// Parse CSS duration strings (e.g., "200ms" or "0.2s") into milliseconds
const parseDurationMs = (val: string): number => {
  const s = (val || "").trim();
  if (!s) return NaN;
  if (s.endsWith("ms")) {
    const v = parseFloat(s.slice(0, -2));
    return isFinite(v) ? v : NaN;
  }
  if (s.endsWith("s")) {
    const v = parseFloat(s.slice(0, -1));
    return isFinite(v) ? Math.round(v * 1000) : NaN;
  }
  const v = parseFloat(s);
  return isFinite(v) ? v : NaN;
};

// Combines layout information from option, column data, and VIZ constants to compute finalized layout metrics.
const computeLayoutMetrics = (
  panel: HTMLElement,
  columnsData: StateColumn[],
  opts: RenderOptions,
): LayoutMetrics => {
  const animationMs = getAnimationMs(panel, opts);

  const columnCount = columnsData.length;
  const minColumnWidthPx = Math.max(
    VIZ.minColumnWidthFloor,
    typeof opts.minColumnWidth === "number"
      ? Math.floor(opts.minColumnWidth)
      : VIZ.defaultMinColumnWidth,
  );
  const maxColumns = opts.maxColumns ?? VIZ.defaultMaxColumns;
  const defaultMinPanelWidthPx = VIZ.defaultMinPanelWidth;
  const defaultMaxPanelWidthPx =
    VIZ.marginLeft +
    VIZ.marginRight +
    maxColumns * (minColumnWidthPx + VIZ.columnSpacing);

  const minWidthPx = Math.max(
    VIZ.minPanelWidthPx,
    opts.minPanelWidthPx ?? defaultMinPanelWidthPx,
  );
  const maxWidthPx = Math.max(
    minWidthPx,
    opts.maxPanelWidthPx ?? defaultMaxPanelWidthPx,
  );
  const growthFactor = Math.max(
    0,
    Math.min(1, (columnCount - 1) / (maxColumns - 1)),
  );
  const panelWidthPx = Math.round(
    minWidthPx + growthFactor * (maxWidthPx - minWidthPx),
  );
  const contentWidthPx = panelWidthPx - VIZ.marginLeft - VIZ.marginRight;
  const columnWidthPx = Math.max(
    4,
    Math.floor(contentWidthPx / Math.max(1, columnCount)) - VIZ.columnSpacing,
  );
  const phaseCircleRadiusPx = Math.max(
    VIZ.phaseRadiusBase,
    Math.floor(columnWidthPx / 2) - 1,
  );
  const maxLabelLen = columnsData.reduce(
    (m, b) => Math.max(m, (displayLabel(b) || "").length),
    0,
  );
  const verticalLabels = maxLabelLen > VIZ.labelLongThresholdChars;

  const maxProb = Math.max(
    VIZ.minProbEpsilon,
    Math.max(...columnsData.map((b) => b.prob ?? 0)),
  );

  const phaseColor = opts.phaseColorMap ?? defaultPhaseColor;
  const othersColor = opts.othersColor ?? VIZ.defaultOthersColor;

  const phaseSectionTopY =
    VIZ.barHeaderPadding + VIZ.barAreaHeight + VIZ.percentLabelPadding;
  const stateSectionTopY =
    phaseSectionTopY +
    VIZ.phaseHeaderPadding +
    2 * phaseCircleRadiusPx +
    VIZ.phaseLabelPadding;

  return {
    panelWidthPx,
    contentWidthPx,
    columnWidthPx,
    maxProb,
    phaseSectionTopY,
    phaseCircleRadiusPx,
    stateSectionTopY,
    verticalLabels,
    maxLabelLen,
    animationMs,
    phaseColor,
    othersColor,
  };
};

// --- Rendering functions ---

const renderStatePanel = (
  panel: HTMLElement,
  columnData: StateColumn[],
  opts: RenderOptions = {},
): void => {
  const svg = panel.querySelector("svg.state-svg") as SVGSVGElement | null;
  if (!svg) return;
  const prev: Record<string, { prob: number; phase: number }> =
    (panel as any)._stateVizPrev ?? {};

  while (svg.firstChild) svg.removeChild(svg.firstChild);

  const layout = computeLayoutMetrics(panel, columnData, opts);

  const g = document.createElementNS("http://www.w3.org/2000/svg", "g");
  g.setAttribute("transform", `translate(${VIZ.marginLeft},${0})`);
  svg.appendChild(g);

  renderSectionHeaders(g, layout);

  columnData.forEach((col, i) => renderColumn(g, col, i, prev, layout));

  finalizeSvgAndFlex(svg, panel, g, layout);
  savePreviousValues(panel, columnData);
};

// Render the section headers and separators
const renderSectionHeaders = (g: SVGGElement, layout: LayoutMetrics) => {
  const mkLabel = (text: string, y: number) => {
    const t = document.createElementNS("http://www.w3.org/2000/svg", "text");
    t.setAttribute("x", `${VIZ.headerLabelXOffset}`);
    t.setAttribute("y", `${y + VIZ.headerLabelYOffset}`);
    t.setAttribute("class", "state-header");
    t.textContent = text;
    return t;
  };
  const mkSep = (y: number) => {
    const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
    line.setAttribute("x1", "0");
    line.setAttribute("y1", `${y}`);
    line.setAttribute("x2", `${layout.contentWidthPx}`);
    line.setAttribute("y2", `${y}`);
    line.setAttribute("class", "state-separator");
    return line;
  };
  g.appendChild(mkLabel("Probability Density", 0));
  g.appendChild(mkSep(layout.phaseSectionTopY));
  g.appendChild(mkLabel("Phase", layout.phaseSectionTopY));
  g.appendChild(mkSep(layout.stateSectionTopY));
  g.appendChild(mkLabel("State", layout.stateSectionTopY));
};

// Render a full column (percentage bar + phase + label)
const renderColumn = (
  g: SVGGElement,
  column: StateColumn,
  colIdx: number,
  prev: Record<string, { prob: number; phase: number }>,
  layout: LayoutMetrics,
) => {
  const colX = colIdx * (layout.columnWidthPx + VIZ.columnSpacing);

  const prevProb = prev[column.label]?.prob ?? 0;
  const bar = renderProbSection(g, column, prevProb, colX, layout);

  const prevPhase = prev[column.label]?.phase ?? 0;
  renderPhaseSection(g, column, prevPhase, bar, colX, layout);

  renderStateLabelSection(g, column, colX, layout);
};

const renderProbSection = (
  g: SVGGElement,
  column: StateColumn,
  prevProb: number,
  colX: number,
  layout: LayoutMetrics,
): SVGRectElement => {
  const { columnWidthPx, maxProb, animationMs, phaseColor, othersColor } =
    layout;
  const cx = colX + columnWidthPx / 2;

  const scaleY = (p: number) =>
    (Math.max(0, Math.min(p, maxProb)) / maxProb) * VIZ.barAreaHeight;

  const bar = document.createElementNS(
    "http://www.w3.org/2000/svg",
    "rect",
  ) as unknown as SVGRectElement;
  bar.setAttribute("x", `${colX}`);
  bar.setAttribute("width", `${columnWidthPx}`);
  bar.setAttribute(
    "fill",
    column.isOthers ? othersColor : phaseColor(column.phase),
  );
  bar.setAttribute("class", "state-bar");
  const tip = document.createElementNS("http://www.w3.org/2000/svg", "title");
  const pctTipTarget = (column.prob ?? 0) * 100;
  tip.textContent = column.isOthers
    ? `${pctTipTarget.toFixed(1)}% • Others (${column.othersCount ?? 0} states)`
    : `${pctTipTarget.toFixed(1)}% • φ=${formatPhasePiTip(column.phase)}`;
  bar.appendChild(tip);
  g.appendChild(bar);

  const fromH = scaleY(prevProb);
  const baseY = VIZ.barHeaderPadding + VIZ.barAreaHeight;
  bar.setAttribute("y", `${baseY - fromH}`);
  bar.setAttribute("height", `${fromH}`);
  animate(prevProb, column.prob, animationMs, (pv) => {
    const h = scaleY(pv);
    bar.setAttribute("y", `${baseY - h}`);
    bar.setAttribute("height", `${h}`);
  });

  const label = document.createElementNS("http://www.w3.org/2000/svg", "text");
  label.setAttribute("x", `${cx}`);
  const labelY =
    VIZ.barHeaderPadding + VIZ.barAreaHeight + VIZ.percentLabelOffset;
  label.setAttribute("y", `${labelY}`);
  label.setAttribute("class", "state-bar-label");
  animate(prevProb, column.prob, animationMs, (pv) => {
    const pct = (pv ?? 0) * 100;
    label.textContent = pct >= 1 ? `${pct.toFixed(0)}%` : `${pct.toFixed(1)}%`;
  });
  g.appendChild(label);

  return bar;
};

const renderPhaseSection = (
  g: SVGGElement,
  column: StateColumn,
  prevPhase: number,
  bar: SVGRectElement,
  colX: number,
  layout: LayoutMetrics,
): void => {
  if (column.isOthers) return;

  const {
    columnWidthPx,
    phaseSectionTopY,
    phaseCircleRadiusPx,
    stateSectionTopY,
    animationMs,
    phaseColor,
  } = layout;
  const cx = colX + columnWidthPx / 2;

  let r = phaseCircleRadiusPx;
  if (r >= VIZ.phaseRadiusThreshold) {
    const maxR = Math.floor(
      (columnWidthPx / 2 - VIZ.phaseCirclePaddingX) / (1 + VIZ.phaseDotFrac),
    );
    r = Math.min(r, Math.max(2, maxR));
  } else {
    const maxR = Math.floor(columnWidthPx / 2 - VIZ.phaseCirclePaddingX - 1.5);
    r = Math.min(r, Math.max(2, maxR));
  }

  const phaseContentYBase = phaseSectionTopY + VIZ.phaseHeaderPadding;
  const cy = phaseContentYBase + r + VIZ.phaseCirclePaddingY;
  const sx = cx + r;
  const sy = cy;
  const ex = cx + r * Math.cos(column.phase);
  const ey = cy - r * Math.sin(column.phase);
  const largeArc = Math.abs(column.phase) > Math.PI ? 1 : 0;
  const sweep = column.phase < 0 ? 1 : 0;

  const wedge = document.createElementNS("http://www.w3.org/2000/svg", "path");
  const dTarget = `M ${cx} ${cy} L ${sx} ${sy} A ${r} ${r} 0 ${largeArc} ${sweep} ${ex} ${ey} Z`;
  wedge.setAttribute("d", dTarget);
  wedge.setAttribute("class", "state-phase-wedge");
  wedge.setAttribute("fill", phaseColor(column.phase));
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
  tipPhase.textContent = `φ=${formatPhasePiTip(column.phase)}`;
  circle.appendChild(tipPhase);
  g.appendChild(circle);

  const phaseText = document.createElementNS(
    "http://www.w3.org/2000/svg",
    "text",
  );
  phaseText.setAttribute("x", `${cx}`);
  const dotRadius = Math.max(VIZ.phaseDotRadiusMinPx, r * VIZ.phaseDotFrac);
  const labelAreaTopY = cy + r + dotRadius;
  const labelAreaBottomY = stateSectionTopY - VIZ.phaseTextBottomPad;
  const availableH = Math.max(0, labelAreaBottomY - labelAreaTopY);
  const textH = VIZ.phaseLabelLineHeight;
  const yTextTop = Math.round(
    labelAreaTopY + Math.max(0, (availableH - textH) / 2),
  );
  phaseText.setAttribute("y", `${yTextTop}`);
  phaseText.setAttribute("class", "state-phase-text");
  animate(prevPhase, column.phase, animationMs, (pv) => {
    phaseText.textContent = formatPhasePi(pv);
  });
  g.appendChild(phaseText);

  const prevDx = r * Math.cos(prevPhase);
  const prevDy = r * Math.sin(prevPhase);
  const dot = document.createElementNS("http://www.w3.org/2000/svg", "circle");
  dot.setAttribute("cx", `${cx + prevDx}`);
  dot.setAttribute("cy", `${cy - prevDy}`);
  dot.setAttribute(
    "r",
    `${Math.max(VIZ.phaseDotRadiusMinPx, r * VIZ.phaseDotFrac)}`,
  );
  dot.setAttribute("fill", phaseColor(column.phase));
  dot.setAttribute("class", "state-phase-dot");
  g.appendChild(dot);

  animate(prevPhase, column.phase, animationMs, (pv) => {
    const dx = r * Math.cos(pv);
    const dy = r * Math.sin(pv);
    dot.setAttribute("cx", `${cx + dx}`);
    dot.setAttribute("cy", `${cy - dy}`);
    const fillColor = phaseColor(pv);
    wedge.setAttribute("fill", fillColor);
    dot.setAttribute("fill", fillColor);
    bar.setAttribute("fill", fillColor);
    const exA = cx + r * Math.cos(pv);
    const eyA = cy - r * Math.sin(pv);
    const largeArcA = Math.abs(pv) > Math.PI ? 1 : 0;
    const sweepA = pv < 0 ? 1 : 0;
    const dA = `M ${cx} ${cy} L ${sx} ${sy} A ${r} ${r} 0 ${largeArcA} ${sweepA} ${exA} ${eyA} Z`;
    wedge.setAttribute("d", dA);
  });
};

const renderStateLabelSection = (
  g: SVGGElement,
  column: StateColumn,
  colX: number,
  layout: LayoutMetrics,
): void => {
  const { columnWidthPx, stateSectionTopY, verticalLabels } = layout;
  const cx = colX + columnWidthPx / 2;

  const stateContentYBase = stateSectionTopY + VIZ.stateHeaderPadding;
  const labelY = verticalLabels
    ? stateContentYBase + VIZ.stateLabelVerticalOffset
    : stateContentYBase + VIZ.stateLabelHorizontalOffset;

  if (verticalLabels) {
    const labelText = displayLabel(column);
    const labelH =
      VIZ.verticalLabelCharHeight * Math.max(1, (labelText || "").length);
    const fo = document.createElementNS(
      "http://www.w3.org/2000/svg",
      "foreignObject",
    );
    fo.setAttribute("x", `${colX}`);
    fo.setAttribute("y", `${labelY}`);
    fo.setAttribute("width", `${columnWidthPx}`);
    fo.setAttribute("height", `${labelH}`);
    const div = document.createElementNS("http://www.w3.org/1999/xhtml", "div");
    div.setAttribute("class", "state-bitstring-fo");
    div.textContent = labelText;
    fo.appendChild(div);
    g.appendChild(fo);
  } else {
    const t = document.createElementNS("http://www.w3.org/2000/svg", "text");
    t.setAttribute("x", `${cx}`);
    t.setAttribute("y", `${labelY}`);
    t.setAttribute("class", "state-bitstring");
    t.textContent = displayLabel(column);
    g.appendChild(t);
  }
};

const finalizeSvgAndFlex = (
  svg: SVGSVGElement,
  panel: HTMLElement,
  g: SVGGElement,
  layout: LayoutMetrics,
) => {
  try {
    const bbox = g.getBBox();
    const contentHeight = Math.ceil(bbox.height + VIZ.contentHeightExtra);
    const svgHeight = Math.max(VIZ.baseHeight, contentHeight);
    svg.setAttribute("height", svgHeight.toString());
    svg.setAttribute("width", layout.panelWidthPx.toString());
    const edgePad = VIZ.edgePad;
    panel.style.flexBasis = `${Math.ceil(layout.panelWidthPx + edgePad)}px`;
  } catch {
    // If getBBox fails (e.g., JSDOM/SVG not fully rendered), fall back to a
    // deterministic height based on our layout constants so snapshots still
    // include the whole visualization.
    const labelTextHeightPx = layout.verticalLabels
      ? VIZ.verticalLabelCharHeight * Math.max(1, layout.maxLabelLen)
      : VIZ.phaseLabelLineHeight;
    const labelBottomY =
      layout.stateSectionTopY +
      VIZ.stateHeaderPadding +
      (layout.verticalLabels
        ? VIZ.stateLabelVerticalOffset
        : VIZ.stateLabelHorizontalOffset) +
      labelTextHeightPx;

    const svgHeight = Math.max(
      VIZ.baseHeight,
      Math.ceil(
        labelBottomY + VIZ.extraBottomPaddingPx + VIZ.marginBottomMinPx,
      ),
    );
    svg.setAttribute("height", svgHeight.toString());
    svg.setAttribute("width", layout.panelWidthPx.toString());
    const edgePad = VIZ.edgePad;
    panel.style.flexBasis = `${Math.ceil(layout.panelWidthPx + edgePad)}px`;
  }
};

const savePreviousValues = (panel: HTMLElement, columnData: StateColumn[]) => {
  const store: Record<string, { prob: number; phase: number }> = {};
  for (const col of columnData)
    store[col.label] = { prob: col.prob, phase: col.phase };
  (panel as any)._stateVizPrev = store;
};

// Simple animation helper for numeric interpolation
const animate = (
  from: number,
  to: number,
  durationMs: number,
  onUpdate: (v: number) => void,
  onDone?: () => void,
) => {
  if (!isFinite(durationMs) || durationMs <= 0) {
    try {
      onUpdate(to);
    } catch {
      // Ignore update errors
    }
    if (onDone) onDone();
    return;
  }
  const start = performance.now();
  const tick = (now: number) => {
    const t = Math.min(1, (now - start) / durationMs);
    const v = from + (to - from) * t;
    try {
      onUpdate(v);
    } catch {
      // Ignore update errors to keep the animation loop alive
    }
    if (t < 1) requestAnimationFrame(tick);
    else if (onDone) onDone();
  };
  requestAnimationFrame(tick);
};

// The default phase color mapping: map phase (-π..π) to HSL hue (0..360)
const defaultPhaseColor = (phi: number) => {
  const hue = ((phi + Math.PI) / (2 * Math.PI)) * 360;
  return `hsl(${hue},70%,50%)`;
};

// Format phase in multiples of π, e.g., -0.5, +0.2
const formatPhasePi = (phi: number): string => {
  const k = phi / Math.PI;
  const sign = k >= 0 ? "+" : "";
  return `${sign}${k.toFixed(1)}`;
};

// Format phase for tooltips, e.g., -0.50π, +0.25π
const formatPhasePiTip = (phi: number): string => {
  const k = phi / Math.PI;
  const sign = k >= 0 ? "+" : "";
  return `${sign}${k.toFixed(2)}π`;
};

// Display label for a state column, handling "Others" case
const displayLabel = (b: StateColumn) =>
  b.isOthers === true ? `Others (${b.othersCount ?? 0})` : b.label;
