// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

export type RenderOptions = {
  maxColumns?: number;
  phaseColorMap?: (phaseRad: number) => string;
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

// Visualization config constants
const VIZ = {
  defaultAnimationMs: 200,
  baseHeight: 80,
  marginLeft: 36,
  marginRight: 36,
  marginBottom: 62,
  minPanelWidthPx: 80,
  defaultMinPanelWidth: 190,
  columnSpacing: 4,
  minColumnWidthFloor: 16,
  defaultMinColumnWidth: 36,
  minMaxColumns: 4,
  defaultMaxColumns: 16,
  labelLongThresholdChars: 4,
  maxColumnsWithLabels: 16,
  barHeaderPadding: 36,
  phaseHeaderPadding: 26,
  stateHeaderPadding: 26,
  percentLabelPadding: 29,
  phaseLabelPadding: 39,
  marginBottomMinPx: 48,
  extraBottomPaddingPx: 24,
  columnAreaHeight: 234,
  minProbEpsilon: 1e-12,
  headerLabelXOffset: -8,
  headerLabelYOffset: 9,
  percentLabelOffset: 8,
  phaseRadiusBase: 12,
  phaseRadiusThreshold: 7.5,
  padX: 2,
  phaseDotFrac: 0.25,
  phaseDotRadiusMinPx: 1.5,
  phaseTextBottomPad: 6,
  phaseContentExtraY: 10,
  verticalLabelLineHeight: 14,
  verticalLabelExtraBase: 12,
  stateLabelVerticalOffset: 4,
  stateLabelHorizontalOffset: 16,
  contentHeightExtra: 10,
  edgePad: 36,
  emptyStateFlexBasisPx: 360,
  rowLabelFallbackPx: 24,
};

// Entry Points

// Adapter: render from a map of named states to complex amplitudes.
export const updateStatePanelFromMap = (
  panel: HTMLElement,
  ampMap: AmpMap,
  opts: RenderOptions & { nQubits?: number } = {},
): void => {
  const entries = Object.entries(ampMap);
  if (entries.length === 0) {
    const svg = panel.querySelector("svg.state-svg") as SVGSVGElement | null;
    if (svg) while (svg.firstChild) svg.removeChild(svg.firstChild);
    return;
  }

  const guessN =
    opts.nQubits ?? entries.reduce((m, [k]) => Math.max(m, k.length), 0);

  // Handle zero-qubit map by showing the empty-state message and hiding SVG
  if (!guessN || guessN <= 0) {
    showEmptyState(panel);
    return;
  }

  // Ensure SVG is visible and remove any empty-state message when rendering data
  showContentState(panel);
  const raw = entries.map(([label, a]) => {
    const { prob, phase } = toPolar(a);
    return { label, prob, phase } as ColumnDatum;
  });
  const doNormalize = opts.normalize ?? true;
  const mass = raw.reduce((total, r) => total + r.prob, 0);
  const states =
    doNormalize && mass > 0
      ? raw.map((r) => ({ ...r, prob: r.prob / mass }))
      : raw;

  const numericRegex = /^[+-]?\d+(?:\.\d+)?$/;
  const asNumber = (s: string) => (numericRegex.test(s) ? parseFloat(s) : NaN);
  const isNumeric = (s: string) => numericRegex.test(s);
  const labelCmp = (a: string, b: string) => a.localeCompare(b);

  const maxColumns = opts.maxColumns ?? 16;
  // Helper comparator for usual label ordering (numeric labels first, then lexical)
  const labelOrderCmp = (a: { label: string }, b: { label: string }) => {
    const an = isNumeric(a.label);
    const bn = isNumeric(b.label);
    if (an && bn) {
      const av = asNumber(a.label);
      const bv = asNumber(b.label);
      return av - bv;
    }
    if (an !== bn) return an ? -1 : 1;
    return labelCmp(a.label, b.label);
  };
  const sortedByLabel = states.slice().sort(labelOrderCmp);
  let columns: ColumnDatum[] = [];
  const minThresh = Math.max(
    0,
    Math.min(
      1,
      typeof opts.minProbThreshold === "number" ? opts.minProbThreshold : 0.0,
    ),
  );
  // Apply threshold against normalized mass so the value represents a percentage of total
  const probForThresh = (r: { prob: number }) =>
    doNormalize || mass <= 0 ? (r.prob ?? 0) : (r.prob ?? 0) / mass;
  const smallStates =
    minThresh > 0 ? states.filter((r) => probForThresh(r) < minThresh) : [];
  const sigStates =
    minThresh > 0
      ? states.filter((r) => probForThresh(r) >= minThresh)
      : states.slice();
  const needOthers =
    smallStates.length > 0 || sortedByLabel.length > maxColumns;
  if (!needOthers) {
    // No need to aggregate; everything fits and no thresholded states
    columns = sortedByLabel;
  } else {
    const reserveOthers = 1; // keep one column for Others when needed
    const capacity = Math.max(0, (opts.maxColumns ?? 16) - reserveOthers);
    // Choose by probability first from significant states (those above threshold)
    const chosenByProb = sigStates
      .slice()
      .sort((a, b) => (b.prob ?? 0) - (a.prob ?? 0))
      .slice(0, capacity);
    const chosenOrdered = chosenByProb.slice().sort(labelOrderCmp);
    const chosenSet = new Set(chosenByProb.map((r) => r.label));
    const tail = states.filter((r) => !chosenSet.has(r.label));
    const othersProb = tail.reduce((s, r) => s + (r.prob ?? 0), 0);
    const othersCount = tail.length;
    if (chosenOrdered.length === 0) {
      // Edge case: threshold or capacity leaves no explicit states; only Others
      columns = [
        {
          label: OTHERS_KEY,
          prob: othersProb,
          phase: 0,
          isOthers: true,
          othersCount,
        },
      ];
    } else {
      columns = [
        ...chosenOrdered,
        {
          label: OTHERS_KEY,
          prob: othersProb,
          phase: 0,
          isOthers: true,
          othersCount,
        },
      ];
    }
  }
  renderStatePanel(panel, columns, opts);
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
    updateStatePanelFromMap(
      panel,
      { [zeros]: { re: 1, im: 0 } },
      { normalize: true, nQubits },
    );
  }
};

export const createStatePanel = (): HTMLElement => {
  const panel = document.createElement("div");
  panel.className = "state-panel";
  // Start collapsed by default
  panel.classList.add("collapsed");

  const edge = document.createElement("div");
  edge.className = "state-edge";
  edge.setAttribute("role", "button");
  edge.setAttribute("tabindex", "0");
  edge.setAttribute("aria-label", "Toggle state panel");
  edge.setAttribute("aria-expanded", "false");
  const edgeText = document.createElement("span");
  edgeText.className = "state-edge-text";
  edgeText.textContent = "State Vizualization";
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

// Data Preparation

const OTHERS_KEY = "__OTHERS__";

export type AmpComplex = { re: number; im: number };
export type AmpPolar = { prob?: number; phase?: number };
export type AmpLike = AmpComplex | AmpPolar;
export type AmpMap = Record<string, AmpLike>;

// Convert amplitude to polar `{ prob, phase }`.
const toPolar = (a: AmpLike): { prob: number; phase: number } => {
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

// Layout Computation

// Render helper that draws the state panel directly from column data
type ColumnDatum = {
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
  columnCount: number;
  maxColumns: number;
  minColumnWidthPx: number;

  maxProb: number;

  phaseSectionTopY: number;
  phaseCircleRadiusPx: number;

  stateSectionTopY: number;
  verticalLabels: boolean;

  animationMs: number;
  scaleY: (p: number) => number;
  clampProb: (p: number) => number;
  displayLabel: (b: ColumnDatum) => string;
  phaseColor: (phi: number) => string;
};

const getAnimationMs = (panel: HTMLElement, opts: RenderOptions): number => {
  let animationMs = VIZ.defaultAnimationMs;
  try {
    const cssDur = getComputedStyle(panel)
      .getPropertyValue("--stateAnimMs")
      .trim();
    const parsed = parseDurationMs(cssDur);
    if (!isNaN(parsed) && parsed > 0) animationMs = parsed;
  } catch {
    void 0;
  }
  if (
    typeof opts.animationMs === "number" &&
    isFinite(opts.animationMs) &&
    opts.animationMs > 0
  ) {
    animationMs = Math.round(opts.animationMs);
  }
  return animationMs;
};

const computeLayoutMetrics = (
  panel: HTMLElement,
  columnsData: ColumnDatum[],
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
  const maxColumns = Math.max(
    VIZ.minMaxColumns,
    opts.maxColumns ?? VIZ.defaultMaxColumns,
  );
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
  const growthFactor = Math.min(
    1,
    Math.max(0, (columnCount - 1) / (maxColumns - 1)),
  );
  const panelWidthPx = Math.round(
    minWidthPx + growthFactor * (maxWidthPx - minWidthPx),
  );
  const contentWidthPx = panelWidthPx - VIZ.marginLeft - VIZ.marginRight;
  const columnWidthPx = Math.max(
    2,
    Math.floor(contentWidthPx / Math.max(1, columnCount)) - VIZ.columnSpacing,
  );
  const phaseCircleRadiusPx = Math.max(
    VIZ.phaseRadiusBase,
    Math.floor(columnWidthPx / 2) - 1,
  );
  const displayLabel = (b: ColumnDatum) =>
    b.label === OTHERS_KEY ? `Others (${b.othersCount ?? 0})` : b.label;
  const maxLabelLen = columnsData.reduce(
    (m, b) => Math.max(m, (displayLabel(b) || "").length),
    0,
  );
  const verticalLabels = maxLabelLen > VIZ.labelLongThresholdChars;
  const barHeaderPaddingPx = VIZ.barHeaderPadding;
  const phaseHeaderPaddingPx = VIZ.phaseHeaderPadding;
  const percentLabelPaddingPx = VIZ.percentLabelPadding;
  const phaseLabelPaddingPx = VIZ.phaseLabelPadding;

  const maxProb = Math.max(
    VIZ.minProbEpsilon,
    Math.max(...columnsData.map((b) => b.prob ?? 0)),
  );
  const barAreaHeightPx = VIZ.columnAreaHeight;
  const scaleY = (p: number) => (p / maxProb) * barAreaHeightPx;
  const clampProb = (p: number) => Math.max(0, Math.min(p, maxProb));
  const phaseColor = opts.phaseColorMap ?? defaultPhaseColor;

  const phaseSectionTopY =
    barHeaderPaddingPx + barAreaHeightPx + percentLabelPaddingPx;
  const stateSectionTopY =
    phaseSectionTopY +
    phaseHeaderPaddingPx +
    2 * phaseCircleRadiusPx +
    phaseLabelPaddingPx;

  return {
    panelWidthPx,
    contentWidthPx,
    columnWidthPx,
    columnCount,
    maxColumns,
    minColumnWidthPx,
    maxProb,
    phaseSectionTopY,
    phaseCircleRadiusPx,
    stateSectionTopY,
    verticalLabels,
    animationMs,
    scaleY,
    clampProb,
    displayLabel,
    phaseColor,
  };
};

// Rendering functions

const renderStatePanel = (
  panel: HTMLElement,
  columnData: ColumnDatum[],
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

// Render a full column (percentage bar + phase + label)
const renderColumn = (
  g: SVGGElement,
  column: ColumnDatum,
  i: number,
  prev: Record<string, { prob: number; phase: number }>,
  layout: LayoutMetrics,
) => {
  const {
    columnWidthPx,
    columnCount,
    phaseSectionTopY,
    phaseCircleRadiusPx,
    stateSectionTopY,
    verticalLabels,
    animationMs,
    scaleY,
    clampProb,
    displayLabel,
    phaseColor,
  } = layout;
  const x = i * (columnWidthPx + VIZ.columnSpacing);
  const bar = document.createElementNS("http://www.w3.org/2000/svg", "rect");
  bar.setAttribute("x", `${x}`);
  bar.setAttribute("width", `${columnWidthPx}`);
  bar.setAttribute(
    "fill",
    column.isOthers ? "#a6a6a6" : phaseColor(column.phase),
  );
  bar.setAttribute("class", "state-bar");
  const tip = document.createElementNS("http://www.w3.org/2000/svg", "title");
  const pctTipTarget = (column.prob ?? 0) * 100;
  tip.textContent = column.isOthers
    ? `${pctTipTarget.toFixed(1)}% • Others (${column.othersCount ?? 0} states)`
    : `${pctTipTarget.toFixed(1)}% • φ=${formatPhasePiTip(column.phase)}`;
  bar.appendChild(tip);
  g.appendChild(bar);

  const prevProb = prev[column.label]?.prob ?? 0;
  const fromH = scaleY(clampProb(prevProb));
  const baseY = VIZ.barHeaderPadding + VIZ.columnAreaHeight;
  bar.setAttribute("y", `${baseY - fromH}`);
  bar.setAttribute("height", `${fromH}`);
  animate(prevProb, column.prob, animationMs, (pv) => {
    const h = scaleY(clampProb(pv));
    bar.setAttribute("y", `${baseY - h}`);
    bar.setAttribute("height", `${h}`);
  });

  if (columnWidthPx >= 4) {
    const label = document.createElementNS(
      "http://www.w3.org/2000/svg",
      "text",
    );
    label.setAttribute("x", `${x + columnWidthPx / 2}`);
    const labelY =
      VIZ.barHeaderPadding + VIZ.columnAreaHeight + VIZ.percentLabelOffset;
    label.setAttribute("y", `${labelY}`);
    label.setAttribute("class", "state-bar-label");
    animate(prevProb, column.prob, animationMs, (pv) => {
      const pct = (pv ?? 0) * 100;
      label.textContent =
        pct >= 1 ? `${pct.toFixed(0)}%` : `${pct.toFixed(1)}%`;
    });
    g.appendChild(label);
  }

  const cx = x + columnWidthPx / 2;
  if (!column.isOthers) {
    const DOT_FRAC = VIZ.phaseDotFrac;
    let r = phaseCircleRadiusPx;
    if (r >= VIZ.phaseRadiusThreshold) {
      const maxR = Math.floor((columnWidthPx / 2 - VIZ.padX) / (1 + DOT_FRAC));
      r = Math.min(r, Math.max(2, maxR));
    } else {
      const maxR = Math.floor(columnWidthPx / 2 - VIZ.padX - 1.5);
      r = Math.min(r, Math.max(2, maxR));
    }
    const phaseContentYBase = phaseSectionTopY + VIZ.phaseHeaderPadding;
    const cy = phaseContentYBase + r + VIZ.phaseContentExtraY;
    const sx = cx + r;
    const sy = cy;
    const ex = cx + r * Math.cos(column.phase);
    const ey = cy - r * Math.sin(column.phase);
    const largeArc = Math.abs(column.phase) > Math.PI ? 1 : 0;
    const sweep = column.phase < 0 ? 1 : 0;
    const wedge = document.createElementNS(
      "http://www.w3.org/2000/svg",
      "path",
    );
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
    const dotRadius = Math.max(VIZ.phaseDotRadiusMinPx, r * DOT_FRAC);
    const yTop = cy + r + dotRadius;
    const yBottom = stateSectionTopY - VIZ.phaseTextBottomPad;
    const textH = VIZ.verticalLabelLineHeight;
    let yTextTop = Math.round((yTop + yBottom) / 2 - textH / 2);
    yTextTop = Math.max(yTop, Math.min(yTextTop, yBottom - textH));
    phaseText.setAttribute("y", `${yTextTop}`);
    phaseText.setAttribute("class", "state-phase-text");
    const prevPhase = prev[column.label]?.phase ?? 0;
    animate(prevPhase, column.phase, animationMs, (pv) => {
      phaseText.textContent = formatPhasePi(pv);
    });
    g.appendChild(phaseText);

    const prevDx = r * Math.cos(prev[column.label]?.phase ?? 0);
    const prevDy = r * Math.sin(prev[column.label]?.phase ?? 0);
    const dot = document.createElementNS(
      "http://www.w3.org/2000/svg",
      "circle",
    );
    dot.setAttribute("cx", `${cx + prevDx}`);
    dot.setAttribute("cy", `${cy - prevDy}`);
    dot.setAttribute("r", `${Math.max(VIZ.phaseDotRadiusMinPx, r * DOT_FRAC)}`);
    dot.setAttribute("fill", phaseColor(column.phase));
    dot.setAttribute("class", "state-phase-dot");
    g.appendChild(dot);

    animate(prev[column.label]?.phase ?? 0, column.phase, animationMs, (pv) => {
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
  }

  if (columnCount <= VIZ.maxColumnsWithLabels) {
    const stateContentYBase = stateSectionTopY + VIZ.stateHeaderPadding;
    const labelX = x + columnWidthPx / 2;
    const labelY = verticalLabels
      ? stateContentYBase + VIZ.stateLabelVerticalOffset
      : stateContentYBase + VIZ.stateLabelHorizontalOffset;

    if (verticalLabels) {
      const lineH = VIZ.verticalLabelLineHeight;
      const labelText = displayLabel(column);
      const labelH = lineH * Math.max(1, (labelText || "").length);
      const fo = document.createElementNS(
        "http://www.w3.org/2000/svg",
        "foreignObject",
      );
      fo.setAttribute("x", `${x}`);
      fo.setAttribute("y", `${labelY}`);
      fo.setAttribute("width", `${columnWidthPx}`);
      fo.setAttribute("height", `${labelH}`);
      const div = document.createElementNS(
        "http://www.w3.org/1999/xhtml",
        "div",
      );
      div.setAttribute("class", "state-bitstring-fo");
      div.textContent = labelText;
      fo.appendChild(div);
      g.appendChild(fo);
    } else {
      const t = document.createElementNS("http://www.w3.org/2000/svg", "text");
      t.setAttribute("x", `${labelX}`);
      t.setAttribute("y", `${labelY}`);
      t.setAttribute("class", "state-bitstring");
      t.textContent = displayLabel(column);
      g.appendChild(t);
    }
  }
};

const renderSectionHeaders = (g: SVGGElement, layout: LayoutMetrics) => {
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
    line.setAttribute("x2", `${layout.contentWidthPx}`);
    line.setAttribute("y2", `${y}`);
    line.setAttribute("class", "state-separator");
    return line;
  };
  const sepBarY = 0;
  g.appendChild(
    mkLabel(
      "Probability Density",
      VIZ.headerLabelXOffset,
      sepBarY + VIZ.headerLabelYOffset,
    ),
  );
  g.appendChild(mkSep(layout.phaseSectionTopY));
  g.appendChild(
    mkLabel(
      "Phase",
      VIZ.headerLabelXOffset,
      layout.phaseSectionTopY + VIZ.headerLabelYOffset,
    ),
  );
  g.appendChild(mkSep(layout.stateSectionTopY));
  g.appendChild(
    mkLabel(
      "State",
      VIZ.headerLabelXOffset,
      layout.stateSectionTopY + VIZ.headerLabelYOffset,
    ),
  );
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
    void 0;
  }
};

const savePreviousValues = (panel: HTMLElement, columnData: ColumnDatum[]) => {
  try {
    const store: Record<string, { prob: number; phase: number }> = {};
    for (const col of columnData)
      store[col.label] = { prob: col.prob, phase: col.phase };
    (panel as any)._stateVizPrev = store;
  } catch {
    void 0;
  }
};

// Simple animation helper for numeric interpolation
const animate = (
  from: number,
  to: number,
  durationMs: number,
  onUpdate: (v: number) => void,
  onDone?: () => void,
) => {
  const start = performance.now();
  const tick = (now: number) => {
    const t = Math.min(1, (now - start) / durationMs);
    const v = from + (to - from) * t;
    try {
      onUpdate(v);
    } catch {
      void 0;
    }
    if (t < 1) requestAnimationFrame(tick);
    else if (onDone) onDone();
  };
  requestAnimationFrame(tick);
};

const showEmptyState = (panel: HTMLElement): void => {
  const svg = panel.querySelector("svg.state-svg") as SVGSVGElement | null;
  if (svg) {
    while (svg.firstChild) svg.removeChild(svg.firstChild);
    try {
      svg.removeAttribute("height");
      (svg as any).style.height = "auto";
    } catch {
      void 0;
    }
    svg.style.display = "none";
  }
  const toolbar = panel.querySelector(".dev-toolbar") as HTMLElement | null;
  if (toolbar) toolbar.style.display = "none";
  ensureEmptyMessage(panel, "The circuit is empty.");
  try {
    if (!panel.classList.contains("collapsed")) {
      panel.style.flexBasis = `${VIZ.emptyStateFlexBasisPx}px`;
    }
  } catch {
    void 0;
  }
};

const showContentState = (panel: HTMLElement): void => {
  const svg = panel.querySelector("svg.state-svg") as SVGSVGElement | null;
  if (svg) svg.style.display = "";
  const toolbar = panel.querySelector(".dev-toolbar") as HTMLElement | null;
  if (toolbar) toolbar.style.display = "";
  const emptyMsg = panel.querySelector(".state-empty-message");
  if (emptyMsg) emptyMsg.remove();
};

// Utilities (formatting, colors, CSS parsing)
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

// Helpers to manage empty/content states without inline styles duplication
const ensureEmptyMessage = (panel: HTMLElement, text: string): void => {
  let msg = panel.querySelector(".state-empty-message") as HTMLElement | null;
  if (!msg) {
    msg = document.createElement("div");
    msg.className = "state-empty-message";
    panel.appendChild(msg);
  }
  msg.textContent = text;
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
