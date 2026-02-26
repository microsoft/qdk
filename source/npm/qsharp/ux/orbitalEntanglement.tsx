// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/**
 * Orbital entanglement chord diagram.
 *
 * Renders single-orbital entropies and mutual information as an SVG chord
 * diagram.  Arc length is proportional to single-orbital entropy; chord
 * thickness is proportional to pairwise mutual information.
 *
 * The diagram is rendered entirely as native SVG so that the markup can be
 * serialised to a standalone `.svg` file from the Python widget.
 */

import { useState, useRef, useEffect } from "preact/hooks";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface OrbitalEntanglementProps {
  /** Single-orbital entropies, length N. */
  s1Entropies: number[];
  /** Mutual information matrix, N×N (row-major flat array or nested). */
  mutualInformation: number[][];
  /** Orbital labels (length N). Falls back to "0", "1", … */
  labels?: string[];
  /** Indices of orbitals to highlight with an outline. */
  selectedIndices?: number[];

  // --- visual knobs (all optional with sensible defaults) ---
  gapDeg?: number;
  radius?: number;
  arcWidth?: number;
  lineScale?: number | null;
  miThreshold?: number;
  s1Vmax?: number | null;
  miVmax?: number | null;
  title?: string | null;
  width?: number;
  height?: number;
  selectionColor?: string;
  selectionLinewidth?: number;
  /**
   * When `true` renders light text on a dark background; when `false`
   * renders dark text on a transparent background.  Leave `undefined`
   * (the default) to inherit from the host page via `currentColor`.
   */
  darkMode?: boolean;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function deg2xy(deg: number, r: number): [number, number] {
  const rad = (deg * Math.PI) / 180;
  return [r * Math.cos(rad), r * Math.sin(rad)];
}

/** Linear interpolation between two RGB‑A colours given as [r,g,b,a]. */
type RGBA = [number, number, number, number];

function lerpColor(a: RGBA, b: RGBA, t: number): RGBA {
  return [
    a[0] + (b[0] - a[0]) * t,
    a[1] + (b[1] - a[1]) * t,
    a[2] + (b[2] - a[2]) * t,
    a[3] + (b[3] - a[3]) * t,
  ];
}

/** Parse "#rrggbb" to RGBA. */
function hexToRGBA(hex: string): RGBA {
  const r = parseInt(hex.slice(1, 3), 16) / 255;
  const g = parseInt(hex.slice(3, 5), 16) / 255;
  const b = parseInt(hex.slice(5, 7), 16) / 255;
  return [r, g, b, 1];
}

function rgbaToCSS(c: RGBA): string {
  return `rgb(${Math.round(c[0] * 255)},${Math.round(c[1] * 255)},${Math.round(c[2] * 255)})`;
}

/** Evaluate a 3‑stop linear colour-map at position t ∈ [0,1]. */
function colormapEval(stops: [string, string, string], t: number): string {
  const clamped = Math.max(0, Math.min(1, t));
  const colors = stops.map(hexToRGBA) as [RGBA, RGBA, RGBA];
  if (clamped <= 0.5) {
    return rgbaToCSS(lerpColor(colors[0], colors[1], clamped * 2));
  }
  return rgbaToCSS(lerpColor(colors[1], colors[2], (clamped - 0.5) * 2));
}

const ARC_CMAP: [string, string, string] = ["#d8d8d8", "#c82020", "#1a1a1a"];
const CHORD_CMAP: [string, string, string] = ["#d8d8d8", "#2060b0", "#1a1a1a"];

/**
 * Detect whether the host background is dark or light by sampling the
 * computed background-color of the nearest ancestor with one.
 * Returns a high-contrast colour for selection outlines.
 */
function detectSelectionColor(el: Element | null): string {
  if (!el || typeof getComputedStyle === "undefined") return "#FFD700";
  let node: Element | null = el;
  while (node) {
    const bg = getComputedStyle(node).backgroundColor;
    if (bg && bg !== "rgba(0, 0, 0, 0)" && bg !== "transparent") {
      const m = bg.match(/\d+/g);
      if (m) {
        const [r, g, b] = m.map(Number);
        const lum = (0.299 * r + 0.587 * g + 0.114 * b) / 255;
        // Use vivid colours that pop against the arc colourmap
        return lum > 0.5 ? "#FF8C00" : "#FFD700";
      }
    }
    node = node.parentElement;
  }
  return "#FFD700";
}

/** Build an SVG arc‑path for a filled annular segment. */
function arcPath(
  startDeg: number,
  endDeg: number,
  innerR: number,
  outerR: number,
): string {
  // Discretise to a polygon — simpler and avoids arc‑sweep flag headaches
  // with very small or very large arcs.
  const N = 80;
  const pts: string[] = [];
  for (let i = 0; i <= N; i++) {
    const theta = ((startDeg + ((endDeg - startDeg) * i) / N) * Math.PI) / 180;
    pts.push(`${outerR * Math.cos(theta)},${outerR * Math.sin(theta)}`);
  }
  for (let i = N; i >= 0; i--) {
    const theta = ((startDeg + ((endDeg - startDeg) * i) / N) * Math.PI) / 180;
    pts.push(`${innerR * Math.cos(theta)},${innerR * Math.sin(theta)}`);
  }
  return (
    `M ${pts[0]} ` +
    pts
      .slice(1)
      .map((p) => `L ${p}`)
      .join(" ") +
    " Z"
  );
}

/** Cubic Bézier chord between two angles on the inner rim. */
function chordPath(
  angleA: number,
  angleB: number,
  radius: number,
  arcWidth: number,
): string {
  const inner = radius - arcWidth;
  const ctrlR = inner * 0.55;
  const [x0, y0] = deg2xy(angleA, inner);
  const [cx0, cy0] = deg2xy(angleA, ctrlR);
  const [cx1, cy1] = deg2xy(angleB, ctrlR);
  const [x1, y1] = deg2xy(angleB, inner);
  return `M ${x0},${y0} C ${cx0},${cy0} ${cx1},${cy1} ${x1},${y1}`;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function OrbitalEntanglement(props: OrbitalEntanglementProps) {
  const {
    s1Entropies,
    mutualInformation,
    labels: labelsProp,
    selectedIndices,
    gapDeg = 3,
    radius = 1,
    arcWidth = 0.08,
    lineScale: lineScaleProp = null,
    miThreshold = 0,
    s1Vmax = null,
    miVmax = null,
    title = "Orbital Entanglement",
    width = 600,
    height = 660,
    selectionColor: selectionColorProp,
    selectionLinewidth = 1.2,
    darkMode,
  } = props;

  // --- theme-resolved colours ---
  const FONT_FAMILY = '"Segoe UI", Roboto, Helvetica, Arial, sans-serif';
  const hasExplicitTheme = darkMode !== undefined;
  const textColor = hasExplicitTheme
    ? darkMode
      ? "#e0e0e0"
      : "#222222"
    : "currentColor";
  const bgColor = hasExplicitTheme
    ? darkMode
      ? "#1e1e1e"
      : "transparent"
    : "transparent";

  const n = s1Entropies.length;

  // --- hover state ---
  const [hoveredIdx, setHoveredIdx] = useState<number | null>(null);

  // --- background-aware selection colour ---
  const svgRef = useRef<SVGSVGElement>(null);
  const [autoSelectionColor, setAutoSelectionColor] = useState("#FFD700");
  useEffect(() => {
    if (svgRef.current) {
      setAutoSelectionColor(detectSelectionColor(svgRef.current));
    }
  }, []);
  const selectionColor = selectionColorProp ?? autoSelectionColor;

  // --- labels ---
  const labels: string[] =
    labelsProp && labelsProp.length === n
      ? labelsProp
      : Array.from({ length: n }, (_, i) => String(i));

  // --- colour scales ---
  const s1Max = s1Vmax ?? Math.log(4);
  const miMax = miVmax ?? Math.log(16);

  const arcColours = s1Entropies.map((v) => colormapEval(ARC_CMAP, v / s1Max));

  // --- line scale ---
  const maxLw = Math.max(12 * (20 / Math.max(n, 1)) ** 0.5, 2);
  let lineScale: number;
  {
    let miPeak = 0;
    for (let i = 0; i < n; i++)
      for (let j = 0; j < n; j++)
        miPeak = Math.max(miPeak, mutualInformation[i][j]);
    if (miPeak <= 0) miPeak = 1;
    lineScale =
      lineScaleProp !== null ? lineScaleProp : maxLw / Math.sqrt(miPeak);
  }

  // --- arc geometry ---
  const totals = s1Entropies.slice();
  let grand = totals.reduce((a, b) => a + b, 0);
  if (grand === 0) {
    totals.fill(1);
    grand = n;
  }
  const gapTotal = gapDeg * n;
  const arcDegs = totals.map((t) => ((360 - gapTotal) * t) / grand);

  const starts: number[] = new Array(n);
  starts[0] = 0;
  for (let i = 1; i < n; i++) {
    starts[i] = starts[i - 1] + arcDegs[i - 1] + gapDeg;
  }

  const arcMids = starts.map((s, i) => s + arcDegs[i] / 2);

  // --- label tiers (avoid overlapping) ---
  const labelFontSize = n <= 20 ? 13.5 : 10.5;
  const maxLabelLen = Math.max(...labels.map((l) => l.length));
  const charDeg = (labelFontSize * 0.7 * maxLabelLen) / Math.max(radius, 0.5);
  const minSepDeg = charDeg * 0.8;
  const baseOffset = 0.07;
  const tierStep = 0.09;
  const maxTiers = 4;

  const indexOrder = Array.from({ length: n }, (_, i) => i).sort(
    (a, b) => arcMids[a] - arcMids[b],
  );
  const tier = new Array(n).fill(0);
  let prevAngle = -999;
  let prevTier = -1;
  for (const idx of indexOrder) {
    const ang = arcMids[idx];
    if (ang - prevAngle < minSepDeg) {
      tier[idx] = (prevTier + 1) % maxTiers;
    } else {
      tier[idx] = 0;
    }
    prevAngle = ang;
    prevTier = tier[idx];
  }
  // wrap‑around
  const firstIdx = indexOrder[0];
  const lastIdx = indexOrder[indexOrder.length - 1];
  const wrapGap = arcMids[firstIdx] + 360 - arcMids[lastIdx];
  if (wrapGap < minSepDeg && tier[firstIdx] === tier[lastIdx]) {
    tier[firstIdx] = (tier[lastIdx] + 1) % maxTiers;
  }

  // --- chord computation ---
  const miRowSums = mutualInformation.map((row) =>
    row.reduce((a, b) => a + b, 0),
  );

  type Conn = { j: number; val: number };
  const nodeConns: Conn[][] = Array.from({ length: n }, () => []);
  for (let i = 0; i < n; i++) {
    for (let j = 0; j < n; j++) {
      if (i === j) continue;
      const val = mutualInformation[i][j];
      if (val <= miThreshold) continue;
      nodeConns[i].push({ j, val });
    }
    const mid = arcMids[i];
    nodeConns[i].sort(
      (a, b) =>
        ((mid - arcMids[a.j] + 360) % 360) - ((mid - arcMids[b.j] + 360) % 360),
    );
  }

  const cursor = starts.slice();
  const allocated = new Map<string, number>();
  for (let i = 0; i < n; i++) {
    for (const { j, val } of nodeConns[i]) {
      const span = miRowSums[i] > 0 ? (arcDegs[i] * val) / miRowSums[i] : 0;
      allocated.set(`${i},${j}`, cursor[i] + span / 2);
      cursor[i] += span;
    }
  }

  type Chord = {
    i: number;
    j: number;
    val: number;
    angleI: number;
    angleJ: number;
  };
  const chords: Chord[] = [];
  for (let i = 0; i < n; i++) {
    for (let j = i + 1; j < n; j++) {
      const keyIJ = `${i},${j}`;
      const keyJI = `${j},${i}`;
      if (!allocated.has(keyIJ)) continue;
      chords.push({
        i,
        j,
        val: mutualInformation[i][j],
        angleI: allocated.get(keyIJ)!,
        angleJ: allocated.get(keyJI)!,
      });
    }
  }
  // lightest first so darkest draws on top
  chords.sort((a, b) => a.val - b.val);

  // --- hover: partition chords ---
  const isHovering = hoveredIdx !== null;
  const bgChords: Chord[] = [];
  const fgChords: Chord[] = [];
  const connectedSet = new Set<number>();
  if (isHovering) {
    for (const ch of chords) {
      if (ch.i === hoveredIdx || ch.j === hoveredIdx) {
        fgChords.push(ch);
        connectedSet.add(ch.i);
        connectedSet.add(ch.j);
      } else {
        bgChords.push(ch);
      }
    }
  }

  // --- selected set ---
  const selectedSet = new Set((selectedIndices ?? []).map(String));

  // --- viewBox ---
  const maxOffset = baseOffset + Math.max(0, ...tier) * tierStep + 0.15;
  const lim = radius + maxOffset;
  // Map [-lim, lim] to [0, width/height] — compact legend area
  const titleH = 50;  // px reserved for title at top
  const legendH = 180; // total height reserved for two colour bars + ticks
  const diagramH = height - legendH - titleH;
  const vbPad = lim * 0.04;
  const vbSize = (lim + vbPad) * 2;
  const scale = Math.min(width, diagramH) / vbSize;

  // Colour-bar dimensions (drawn inside the SVG, close to diagram)
  const cbGap = 40; // px between diagram bottom and first bar
  const cbY = titleH + diagramH + cbGap;
  const cbW = width * 0.6;
  const cbX = (width - cbW) / 2;
  const cbH = 10;
  const cbSpacing = 68; // vertical distance between the two bars (label + bar + ticks)
  const numCbStops = 64;
  const numTicks = 5; // tick count on each colour bar

  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width={width}
      height={height}
      class="qs-orbital-entanglement"
      style={{ background: bgColor, fontFamily: FONT_FAMILY }}
      ref={svgRef}
    >
      {/* Title */}
      {title && (
        <text
          x={width / 2}
          y={28}
          text-anchor="middle"
          font-size="21"
          font-weight="bold"
          fill={textColor}
        >
          {title}
        </text>
      )}

      {/* Diagram group — centred and scaled to fit */}
      <g
        transform={`translate(${width / 2},${titleH + diagramH / 2}) scale(${scale})`}
      >
        {/* Chord lines — when hovering, split into dimmed background + bright foreground */}
        {(isHovering ? bgChords : chords).map((ch, ci) => {
          const c = colormapEval(CHORD_CMAP, ch.val / miMax);
          const lwPx = Math.min(Math.sqrt(ch.val) * lineScale, maxLw);
          const lw = lwPx / scale;
          return (
            <path
              key={`chord-bg-${ci}`}
              d={chordPath(ch.angleI, ch.angleJ, radius, arcWidth)}
              fill="none"
              stroke={c}
              stroke-width={lw}
              stroke-linecap="round"
              opacity={isHovering ? 0.12 : 1}
            />
          );
        })}
        {/* Highlighted chords for hovered orbital (drawn on top) */}
        {fgChords.map((ch, ci) => {
          const c = colormapEval(CHORD_CMAP, ch.val / miMax);
          const lwPx = Math.min(Math.sqrt(ch.val) * lineScale, maxLw);
          const lw = lwPx / scale;
          return (
            <path
              key={`chord-fg-${ci}`}
              d={chordPath(ch.angleI, ch.angleJ, radius, arcWidth)}
              fill="none"
              stroke={c}
              stroke-width={Math.max(lw, 1.5 / scale)}
              stroke-linecap="round"
              opacity={1}
            />
          );
        })}

        {/* Arcs */}
        {Array.from({ length: n }, (_, i) => (
          <path
            key={`arc-${i}`}
            d={arcPath(
              starts[i],
              starts[i] + arcDegs[i],
              radius - arcWidth,
              radius,
            )}
            fill={arcColours[i]}
            opacity={isHovering && !connectedSet.has(i) ? 0.25 : 1}
            onMouseEnter={() => setHoveredIdx(i)}
            onMouseLeave={() => setHoveredIdx(null)}
            style={{ cursor: "pointer" }}
          />
        ))}

        {/* Selection outlines */}
        {Array.from({ length: n }, (_, i) =>
          selectedSet.has(labels[i]) ? (
            <path
              key={`sel-${i}`}
              d={arcPath(
                starts[i],
                starts[i] + arcDegs[i],
                radius - arcWidth,
                radius,
              )}
              fill="none"
              stroke={selectionColor}
              stroke-width={selectionLinewidth / scale}
              style={{ pointerEvents: "none" }}
            />
          ) : null,
        )}

        {/* Labels & tick lines */}
        {Array.from({ length: n }, (_, i) => {
          const mid = arcMids[i];
          const t = tier[i];
          const offset = baseOffset + t * tierStep;
          const [lx, ly] = deg2xy(mid, radius + offset);
          const angle = mid % 360;
          const ha = angle > 90 && angle < 270 ? "end" : "start";
          const rot = angle > 90 && angle < 270 ? angle - 180 : angle;

          const tickLine =
            t > 0
              ? (() => {
                  const [rx, ry] = deg2xy(mid, radius + 0.01);
                  return (
                    <line
                      x1={rx}
                      y1={ry}
                      x2={lx}
                      y2={ly}
                      stroke="#aaaaaa"
                      stroke-width={0.5 / scale}
                    />
                  );
                })()
              : null;

          // Font size in SVG user units — we're in a scaled group so
          // approximate by dividing the pt size by the scale factor.
          const fsPx = labelFontSize / scale;

          // When hovering, replace the plain label with value info
          const isThisHovered = hoveredIdx === i;
          const isConnected = connectedSet.has(i);
          let labelText = labels[i];
          let labelOpacity = 1;
          if (isHovering) {
            if (isThisHovered) {
              labelText = `${labels[i]}  S\u2081=${s1Entropies[i].toFixed(3)}`;
            } else if (isConnected && hoveredIdx !== null) {
              labelText = `${labels[i]}  MI=${mutualInformation[hoveredIdx][i].toFixed(3)}`;
            } else {
              labelOpacity = 0.15;
            }
          }

          return (
            <g key={`label-${i}`} opacity={labelOpacity}>
              {tickLine}
              <text
                x={lx}
                y={ly}
                text-anchor={ha}
                dominant-baseline="central"
                font-size={fsPx}
                font-weight="bold"
                fill={textColor}
                transform={`rotate(${rot},${lx},${ly})`}
              >
                {labelText}
              </text>
            </g>
          );
        })}
      </g>

      {/* ---- Colour-bar legends ---- */}
      {/* Arc (entropy) colour bar */}
      <g>
        <text
          x={width / 2}
          y={cbY - 6}
          text-anchor="middle"
          font-size="18"
          fill={textColor}
        >
          Single-orbital entropy
        </text>
        {Array.from({ length: numCbStops }, (_, k) => {
          const t = k / (numCbStops - 1);
          return (
            <rect
              key={`cb-arc-${k}`}
              x={cbX + (cbW * k) / numCbStops}
              y={cbY}
              width={cbW / numCbStops + 0.5}
              height={cbH}
              fill={colormapEval(ARC_CMAP, t)}
            />
          );
        })}
        {/* Ticks */}
        {Array.from({ length: numTicks }, (_, k) => {
          const frac = k / (numTicks - 1);
          const xPos = cbX + cbW * frac;
          const val = s1Max * frac;
          return (
            <g key={`cb-arc-tick-${k}`}>
              <line
                x1={xPos}
                y1={cbY + cbH}
                x2={xPos}
                y2={cbY + cbH + 3}
                stroke={textColor}
                stroke-width={0.5}
              />
              <text
                x={xPos}
                y={cbY + cbH + 14}
                text-anchor="middle"
                font-size="14"
                fill={textColor}
              >
                {val.toFixed(2)}
              </text>
            </g>
          );
        })}
      </g>

      {/* Chord (MI) colour bar */}
      <g>
        <text
          x={width / 2}
          y={cbY + cbH + cbSpacing - 6}
          text-anchor="middle"
          font-size="18"
          fill={textColor}
        >
          Mutual information
        </text>
        {Array.from({ length: numCbStops }, (_, k) => {
          const t = k / (numCbStops - 1);
          return (
            <rect
              key={`cb-mi-${k}`}
              x={cbX + (cbW * k) / numCbStops}
              y={cbY + cbH + cbSpacing}
              width={cbW / numCbStops + 0.5}
              height={cbH}
              fill={colormapEval(CHORD_CMAP, t)}
            />
          );
        })}
        {/* Ticks */}
        {Array.from({ length: numTicks }, (_, k) => {
          const frac = k / (numTicks - 1);
          const xPos = cbX + cbW * frac;
          const val = miMax * frac;
          return (
            <g key={`cb-mi-tick-${k}`}>
              <line
                x1={xPos}
                y1={cbY + cbH + cbSpacing + cbH}
                x2={xPos}
                y2={cbY + cbH + cbSpacing + cbH + 3}
                stroke={textColor}
                stroke-width={0.5}
              />
              <text
                x={xPos}
                y={cbY + cbH + cbSpacing + cbH + 14}
                text-anchor="middle"
                font-size="14"
                fill={textColor}
              >
                {val.toFixed(2)}
              </text>
            </g>
          );
        })}
      </g>
    </svg>
  );
}
