// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

export type RenderOptions = {
  maxBars?: number;
  heightPx?: number;
  widthPx?: number;
  phaseColorMap?: (phaseRad: number) => string;
  normalize?: boolean; // normalize probabilities to unit mass (default true)
  minBarWidth?: number; // minimum width per bar to avoid label collisions
  barSpacingPx?: number; // horizontal spacing between bars (default 3)
  minPanelWidthPx?: number; // prescribed minimum panel width in pixels
  maxPanelWidthPx?: number; // prescribed maximum panel width in pixels
  uiScale?: number; // global UI scale multiplier (default 1)
  animationMs?: number; // global animation duration in ms (default 200ms)
};

const _defaultPhaseColor = (phi: number) => {
  const hue = ((phi + Math.PI) / (2 * Math.PI)) * 360;
  return `hsl(${hue},70%,50%)`;
};

// Format phase in multiples of π, e.g., -0.5, +0.2
const _formatPhasePi = (phi: number): string => {
  const k = phi / Math.PI;
  const sign = k >= 0 ? "+" : "";
  return `${sign}${k.toFixed(1)}`;
};

// Format phase for tooltips, e.g., -0.50π, +0.25π
const _formatPhasePiTip = (phi: number): string => {
  const k = phi / Math.PI;
  const sign = k >= 0 ? "+" : "";
  return `${sign}${k.toFixed(2)}π`;
};

// Simplified input: map of state name -> amplitude.
export type AmpComplex = { re: number; im: number };
export type AmpPolar = { prob?: number; phase?: number };
export type AmpLike = AmpComplex | AmpPolar;
export type AmpMap = Record<string, AmpLike>;

// Convert amplitude to polar `{ prob, phase }`.
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

// Simple animation helper for numeric interpolation
const _animate = (
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

// Parse CSS duration strings (e.g., "200ms" or "0.2s") into milliseconds
const _parseDurationMs = (val: string): number => {
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

// Helper: create an AmpMap from polar entries
export const toAmpMapPolar = (
  items: Array<{ bit: string; prob?: number; phase?: number }>,
): AmpMap => {
  const m: AmpMap = {};
  for (const it of items) {
    m[it.bit] = { prob: it.prob ?? 0, phase: it.phase ?? 0 };
  }
  return m;
};

// Static mock map with a few non-zero amplitudes; other states are implicitly zero.
export const getStaticMockAmpMap = (setNum: number): AmpMap => {
  switch (setNum % 4) {
    case 0:
      return staticMockAmp1();
    case 1:
      return staticMockAmp2();
    case 2:
      return staticMockAmp3();
    case 3:
      return staticMockAmp4();
    default:
      return {};
  }
};

const staticMockAmp1 = (): AmpMap => {
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

  return ampMap;
};

const staticMockAmp2 = (): AmpMap => {
  const ampMap = staticMockAmp1();

  delete ampMap["000"];
  delete ampMap["001"];
  delete ampMap["010"];
  delete ampMap["100"];
  delete ampMap["101"];
  delete ampMap["110"];
  delete ampMap["111"];

  return ampMap;
};

const staticMockAmp3 = (): AmpMap => {
  const ampMap = staticMockAmp1();

  delete ampMap["000"];
  delete ampMap["001"];
  delete ampMap["010"];
  delete ampMap["100"];
  delete ampMap["110"];
  delete ampMap["111"];

  return ampMap;
};

const staticMockAmp4 = (): AmpMap => {
  return {
    A: { prob: 0.6, phase: 0 },
    B: { prob: 0.6, phase: 0 },
    C: { prob: 0.6, phase: 0 },
    D: { prob: 0.6, phase: 0 },
    E: { prob: 0.6, phase: 0 },
    F: { prob: 0.6, phase: 0 },
    G: { prob: 0.6, phase: 0 },
    H: { prob: 0.6, phase: 0 },
    I: { prob: 0.6, phase: 0 },
    J: { prob: 0.6, phase: 0 },
    K: { prob: 0.6, phase: 0 },
    L: { prob: 0.6, phase: 0 },
    M: { prob: 0.6, phase: 0 },
    N: { prob: 0.6, phase: 0 },
    O: { prob: 0.6, phase: 0 },
    P: { prob: 0.6, phase: 0 },
    Q: { prob: 0.6, phase: 0 },
    R: { prob: 0.6, phase: 0 },
    S: { prob: 0.6, phase: 0 },
    T: { prob: 0.6, phase: 0 },
  };
};

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
    const svg = panel.querySelector("svg.state-svg") as SVGSVGElement | null;
    if (svg) {
      while (svg.firstChild) svg.removeChild(svg.firstChild);
      svg.style.display = "none";
    }
    const toolbar = panel.querySelector(".state-toolbar") as HTMLElement | null;
    if (toolbar) toolbar.style.display = "none";
    let msg = panel.querySelector(".state-empty-message") as HTMLElement | null;
    if (!msg) {
      msg = document.createElement("div");
      msg.className = "state-empty-message";
      msg.textContent = "The circuit is empty.";
      msg.style.position = "absolute";
      msg.style.top = "50%";
      msg.style.left = "50%";
      msg.style.transform = "translate(-50%, -50%)";
      msg.style.padding = "8px";
      msg.style.textAlign = "center";
      msg.style.fontSize = "13px";
      msg.style.color = "#666";
      msg.style.zIndex = "20";
      msg.style.pointerEvents = "none";
      panel.appendChild(msg);
    }
    return;
  }

  // Ensure SVG is visible and remove any empty-state message when rendering data
  const svgEnsure = panel.querySelector(
    "svg.state-svg",
  ) as SVGSVGElement | null;
  if (svgEnsure) svgEnsure.style.display = "";
  const toolbar = panel.querySelector(".state-toolbar") as HTMLElement | null;
  if (toolbar) toolbar.style.display = "";
  const emptyMsg = panel.querySelector(".state-empty-message");
  if (emptyMsg) emptyMsg.remove();
  const raw = entries.map(([bit, a]) => {
    const { prob, phase } = _toPolar(a);
    return { bit, prob, phase };
  });
  const doNormalize = opts.normalize ?? true;
  const mass = raw.reduce((s, r) => s + r.prob, 0);
  const states =
    doNormalize && mass > 0
      ? raw.map((r) => ({ ...r, prob: r.prob / mass }))
      : raw;

  const numericRegex = /^[+-]?\d+(?:\.\d+)?$/;
  const asNumber = (s: string) => (numericRegex.test(s) ? parseFloat(s) : NaN);
  const isNumeric = (s: string) => numericRegex.test(s);
  const labelCmp = (a: string, b: string) => a.localeCompare(b);

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
    svg.style.display = "none";
    const toolbar = panel.querySelector(".state-toolbar") as HTMLElement | null;
    if (toolbar) toolbar.style.display = "none";
    let msg = panel.querySelector(".state-empty-message") as HTMLElement | null;
    if (!msg) {
      msg = document.createElement("div");
      msg.className = "state-empty-message";
      msg.textContent = "The circuit is empty.";
      msg.style.position = "absolute";
      msg.style.top = "50%";
      msg.style.left = "50%";
      msg.style.transform = "translate(-50%, -50%)";
      msg.style.padding = "8px";
      msg.style.textAlign = "center";
      msg.style.fontSize = "13px";
      msg.style.color = "#666";
      msg.style.zIndex = "20";
      msg.style.pointerEvents = "none";
      panel.appendChild(msg);
    }
  } else {
    // Remove message and render the deterministic zero-state
    const msg = panel.querySelector(".state-empty-message");
    if (msg) msg.remove();
    const toolbar = panel.querySelector(".state-toolbar") as HTMLElement | null;
    if (toolbar) toolbar.style.display = "";
    svg.style.display = "";
    const zeros = "0".repeat(nQubits);
    updateStatePanelFromMap(
      panel,
      { [zeros]: { re: 1, im: 0 } },
      { normalize: true, nQubits },
    );
  }
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
  const prev: Record<string, { prob: number; phase: number }> =
    (panel as any)._stateVizPrev ?? {};
  let s = 1;
  try {
    const v = getComputedStyle(panel).getPropertyValue("--stateScale").trim();
    if (v) {
      const parsed = parseFloat(v);
      if (!isNaN(parsed) && isFinite(parsed)) s = parsed;
    }
  } catch {
    void 0;
  }
  if (opts.uiScale && isFinite(opts.uiScale)) s = opts.uiScale;

  // Resolve global animation duration (opts overrides CSS variable; defaults to 200ms)
  let animMs = 200;
  try {
    const cssDur = getComputedStyle(panel)
      .getPropertyValue("--stateAnimMs")
      .trim();
    const parsed = _parseDurationMs(cssDur);
    if (!isNaN(parsed) && parsed > 0) animMs = parsed;
  } catch {
    void 0;
  }
  if (
    typeof opts.animationMs === "number" &&
    isFinite(opts.animationMs) &&
    opts.animationMs > 0
  ) {
    animMs = Math.round(opts.animationMs);
  }
  const height =
    svg.clientHeight ||
    (opts.heightPx ? Math.round(opts.heightPx * s) : Math.round(338 * s));
  const margin = {
    top: 0,
    right: Math.round(36 * s),
    bottom: Math.round(62 * s),
    left: Math.round(36 * s),
  };

  while (svg.firstChild) svg.removeChild(svg.firstChild);

  const n = barsData.length;
  const spacing = Math.max(1, Math.floor((opts.barSpacingPx ?? 4) * s));
  const baseMinBar = opts.minBarWidth ?? 36;
  const minBarWidth = Math.max(Math.floor(16 * s), Math.floor(baseMinBar * s));
  const maxCols = Math.max(4, opts.maxBars ?? 16);
  const defaultMinWidth = Math.floor(190 * s);
  const defaultMaxWidth =
    margin.left + margin.right + maxCols * (minBarWidth + spacing);

  const minWidthPx = Math.max(80, opts.minPanelWidthPx ?? defaultMinWidth);
  const maxWidthPx = Math.max(
    minWidthPx,
    opts.maxPanelWidthPx ?? defaultMaxWidth,
  );
  const growthFactor = Math.min(1, Math.max(0, (n - 1) / (maxCols - 1)));
  const width = Math.round(
    minWidthPx + growthFactor * (maxWidthPx - minWidthPx),
  );
  const wTemp = width - margin.left - margin.right;
  const bw = Math.max(2, Math.floor(wTemp / Math.max(1, n)) - spacing);
  const rCol = Math.max(Math.round(12 * s), Math.floor(bw / 2) - 1);
  const extraForBits = n <= 16 ? Math.round(24 * s) : 0;
  const barHeaderSpace = Math.round(36 * s);
  const phaseHeaderSpace = Math.round(26 * s);
  const stateHeaderSpace = Math.round(26 * s);
  const barLabelSpace = Math.round(29 * s);
  const phaseLabelSpace = Math.round(39 * s);
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
  const hBars = Math.round(234 * s);
  const scaleY = (p: number) => (p / maxProb) * hBars;
  const clampProb = (p: number) => Math.max(0, Math.min(p, maxProb));
  const DOT_FRAC = 0.25; // dot radius as a fraction of phase circle radius

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
    bar.setAttribute("width", `${bw}`);
    bar.setAttribute("fill", phaseColor(b.phase));
    bar.setAttribute("class", "state-bar");
    const tip = document.createElementNS("http://www.w3.org/2000/svg", "title");
    const pctTipTarget = (b.prob ?? 0) * 100;
    tip.textContent = `${pctTipTarget.toFixed(1)}% • φ=${_formatPhasePiTip(b.phase)}`;
    bar.appendChild(tip);
    g.appendChild(bar);

    // Animate probability bar from previous prob to new prob
    const prevProb = prev[b.bit]?.prob ?? 0;
    const fromH = scaleY(clampProb(prevProb));
    const baseY = barHeaderSpace + hBars;
    bar.setAttribute("y", `${baseY - fromH}`);
    bar.setAttribute("height", `${fromH}`);
    _animate(prevProb, b.prob, animMs, (pv) => {
      const h = scaleY(clampProb(pv));
      bar.setAttribute("y", `${baseY - h}`);
      bar.setAttribute("height", `${h}`);
    });

    if (bw >= 4) {
      const label = document.createElementNS(
        "http://www.w3.org/2000/svg",
        "text",
      );
      label.setAttribute("x", `${x + bw / 2}`);
      const labelY = barHeaderSpace + hBars + Math.round(8 * s);
      label.setAttribute("y", `${labelY}`);
      label.setAttribute("class", "state-bar-label");
      // Animate percentage text along with bar height
      _animate(prevProb, b.prob, animMs, (pv) => {
        const pct = (pv ?? 0) * 100;
        label.textContent =
          pct >= 1 ? `${pct.toFixed(0)}%` : `${pct.toFixed(1)}%`;
      });
      g.appendChild(label);
    }

    const cx = x + bw / 2;
    const padX = Math.max(2, Math.round(2 * s));
    let r = rCol;
    // Clamp radius so the phase dot stays within the column bounds
    // Constraint: r + dotR <= bw/2 - padX, where dotR = max(1.5, 0.2 * r)
    if (r >= 7.5) {
      const maxR = Math.floor((bw / 2 - padX) / (1 + DOT_FRAC));
      r = Math.min(r, Math.max(2, maxR));
    } else {
      const maxR = Math.floor(bw / 2 - padX - 1.5);
      r = Math.min(r, Math.max(2, maxR));
    }
    const phaseContentYBase = sepPhaseY + phaseHeaderSpace;
    const cy = phaseContentYBase + r + Math.round(10 * s);
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
    const dTarget = `M ${cx} ${cy} L ${sx} ${sy} A ${r} ${r} 0 ${largeArc} ${sweep} ${ex} ${ey} Z`;
    wedge.setAttribute("d", dTarget);
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
    tipPhase.textContent = `φ=${_formatPhasePiTip(b.phase)}`;
    circle.appendChild(tipPhase);
    g.appendChild(circle);

    const phaseText = document.createElementNS(
      "http://www.w3.org/2000/svg",
      "text",
    );
    phaseText.setAttribute("x", `${cx}`);
    // Center phase label between the bottom of the circle (including dot)
    // and the separator line below the phase section, accounting for text height
    // because dominant-baseline is set to 'hanging' (y is the top of the text box).
    const dotRadius = Math.max(1.5, r * DOT_FRAC);
    const yTop = cy + r + dotRadius;
    const yBottom = sepStateY - Math.round(6 * s); // small margin above separator
    const textH = Math.round(14 * s); // matches CSS font-size for .state-phase-text
    // Compute the top of the text box so the text is visually centered in the region
    let yTextTop = Math.round((yTop + yBottom) / 2 - textH / 2);
    // Clamp to stay within available region
    yTextTop = Math.max(yTop, Math.min(yTextTop, yBottom - textH));
    phaseText.setAttribute("y", `${yTextTop}`);
    phaseText.setAttribute("class", "state-phase-text");
    // Animate phase text from previous phase to new
    const prevPhase = prev[b.bit]?.phase ?? 0;
    _animate(prevPhase, b.phase, animMs, (pv) => {
      phaseText.textContent = _formatPhasePi(pv);
    });
    g.appendChild(phaseText);

    const prevDx = r * Math.cos(prev[b.bit]?.phase ?? 0);
    const prevDy = r * Math.sin(prev[b.bit]?.phase ?? 0);
    const dot = document.createElementNS(
      "http://www.w3.org/2000/svg",
      "circle",
    );
    dot.setAttribute("cx", `${cx + prevDx}`);
    dot.setAttribute("cy", `${cy - prevDy}`);
    dot.setAttribute("r", `${Math.max(1.5, r * DOT_FRAC)}`);
    dot.setAttribute("fill", phaseColor(b.phase));
    dot.setAttribute("class", "state-phase-dot");
    g.appendChild(dot);

    // Animate dot position and wedge color along the phase change
    _animate(prev[b.bit]?.phase ?? 0, b.phase, animMs, (pv) => {
      const dx = r * Math.cos(pv);
      const dy = r * Math.sin(pv);
      dot.setAttribute("cx", `${cx + dx}`);
      dot.setAttribute("cy", `${cy - dy}`);
      const fillColor = phaseColor(pv);
      wedge.setAttribute("fill", fillColor);
      dot.setAttribute("fill", fillColor);
      bar.setAttribute("fill", fillColor);
      // also animate wedge arc path to follow phase
      const exA = cx + r * Math.cos(pv);
      const eyA = cy - r * Math.sin(pv);
      const largeArcA = Math.abs(pv) > Math.PI ? 1 : 0;
      const sweepA = pv < 0 ? 1 : 0;
      const dA = `M ${cx} ${cy} L ${sx} ${sy} A ${r} ${r} 0 ${largeArcA} ${sweepA} ${exA} ${eyA} Z`;
      wedge.setAttribute("d", dA);
    });

    if (n <= 16) {
      const t = document.createElementNS("http://www.w3.org/2000/svg", "text");
      t.setAttribute("x", `${x + bw / 2}`);
      const stateContentYBase = sepStateY + stateHeaderSpace;
      t.setAttribute("y", `${stateContentYBase + Math.round(16 * s)}`);
      t.setAttribute("class", "state-bitstring");
      t.textContent = b.bit;
      g.appendChild(t);
    }
  });

  try {
    const bbox = g.getBBox();
    const svgHeight = Math.max(
      height,
      Math.ceil(bbox.height + margin.top + Math.round(10 * s)),
    );
    svg.setAttribute("height", svgHeight.toString());
    svg.setAttribute("width", width.toString());
    const edgePad = Math.round(36 * s);
    if (!panel.classList.contains("collapsed")) {
      panel.style.flexBasis = `${Math.ceil(width + edgePad)}px`;
    }
  } catch {
    void 0;
  }

  // Save current values for smooth animation on next update
  try {
    const store: Record<string, { prob: number; phase: number }> = {};
    for (const b of barsData) store[b.bit] = { prob: b.prob, phase: b.phase };
    (panel as any)._stateVizPrev = store;
  } catch {
    void 0;
  }
};

export const createStatePanel = (): HTMLElement => {
  const panel = document.createElement("div");
  panel.className = "state-panel";

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
