// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/**
 * Orbital entanglement chord diagram.
 *
 * Renders single-orbital entropies and mutual information as an SVG chord
 * diagram.  Arc length is proportional to single-orbital entropy; chord
 * thickness is proportional to pairwise mutual information.
 */

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
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function deg2xy(deg: number, r: number): [number, number] {
  const rad = (deg * Math.PI) / 180;
  return [r * Math.cos(rad), r * Math.sin(rad)];
}

/** Linear interpolation between two RGB‑A colors given as [r,g,b,a]. */
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

/** Evaluate a 3‑stop linear color-map at position t ∈ [0,1]. */
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
    selectionColor = "var(--qdk-focus-border)",
    selectionLinewidth = 2.5,
  } = props;

  const n = s1Entropies.length;

  // --- labels ---
  const labels: string[] =
    labelsProp && labelsProp.length === n
      ? labelsProp
      : Array.from({ length: n }, (_, i) => String(i));

  // --- color scales ---
  const s1Max = s1Vmax ?? Math.log(4);
  const miMax = miVmax ?? Math.log(16);

  const arccolors = s1Entropies.map((v) => colormapEval(ARC_CMAP, v / s1Max));

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
  const labelFontSize = n <= 20 ? 9 : 7;
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

  type Chord = { val: number; angleI: number; angleJ: number };
  const chords: Chord[] = [];
  for (let i = 0; i < n; i++) {
    for (let j = i + 1; j < n; j++) {
      const keyIJ = `${i},${j}`;
      const keyJI = `${j},${i}`;
      if (!allocated.has(keyIJ)) continue;
      chords.push({
        val: mutualInformation[i][j],
        angleI: allocated.get(keyIJ)!,
        angleJ: allocated.get(keyJI)!,
      });
    }
  }
  // lightest first so darkest draws on top
  chords.sort((a, b) => a.val - b.val);

  // --- selected set ---
  const selectedSet = new Set(selectedIndices ?? []);

  // --- viewBox ---
  const maxOffset = baseOffset + Math.max(0, ...tier) * tierStep + 0.15;
  const lim = radius + maxOffset;
  // Map [-lim, lim] to [0, width/height] with some padding for color bars
  const diagramH = height - 60; // leave room for legends
  const vbPad = lim * 0.05;
  const vbSize = (lim + vbPad) * 2;

  // color-bar dimensions (drawn inside the SVG)
  const cbY = diagramH + 8;
  const cbW = width * 0.6;
  const cbX = (width - cbW) / 2;
  const cbH = 12;
  const numCbStops = 64;

  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox={`0 0 ${width} ${height}`}
      width="100%"
      class="qs-orbital-entanglement"
    >
      {/* Title */}
      {title && (
        <text x={width / 2} y={20} class="qs-orbital-entanglement-title">
          {title}
        </text>
      )}

      {/* Diagram group — centred and scaled to fit */}
      <g
        transform={`translate(${width / 2},${(diagramH + 30) / 2}) scale(${Math.min(width, diagramH) / vbSize / 2})`}
      >
        {/* Chord lines (lightest → darkest) */}
        {chords.map((ch, ci) => {
          const c = colormapEval(CHORD_CMAP, ch.val / miMax);
          const lw = Math.min(Math.sqrt(ch.val) * lineScale, maxLw);
          return (
            <path
              key={`chord-${ci}`}
              d={chordPath(ch.angleI, ch.angleJ, radius, arcWidth)}
              fill="none"
              stroke={c}
              stroke-width={lw}
              stroke-linecap="round"
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
            fill={arccolors[i]}
          />
        ))}

        {/* Selection outlines */}
        {Array.from({ length: n }, (_, i) =>
          selectedSet.has(i) ? (
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
              stroke-width={selectionLinewidth / 100}
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
                      class="qs-orbital-entanglement-label-tick"
                    />
                  );
                })()
              : null;

          // Font size in SVG user units — we're in a scaled group so
          // approximate by dividing the pt size by the scale factor.
          const fsPx = labelFontSize / (Math.min(width, diagramH) / vbSize / 2);

          return (
            <g key={`label-${i}`}>
              {tickLine}
              <text
                class={`qs-orbital-entanglement-label ${ha === "end" ? "qs-orbital-entanglement-label-end" : "qs-orbital-entanglement-label-start"}`}
                x={lx}
                y={ly}
                font-size={fsPx}
                transform={`rotate(${rot},${lx},${ly})`}
              >
                {labels[i]}
              </text>
            </g>
          );
        })}
      </g>

      {/* ---- color-bar legends ---- */}
      {/* Arc (entropy) color bar */}
      <g>
        <text x={width / 2} y={cbY - 2} class="qs-orbital-entanglement-legend-title">
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
        <text x={cbX} y={cbY + cbH + 10} class="qs-orbital-entanglement-legend-value">
          0
        </text>
        <text
          x={cbX + cbW}
          y={cbY + cbH + 10}
          class="qs-orbital-entanglement-legend-value qs-orbital-entanglement-legend-value-end"
        >
          {s1Max.toFixed(2)}
        </text>
      </g>

      {/* Chord (MI) color bar */}
      <g>
        <text
          x={width / 2}
          y={cbY + cbH + 22}
          class="qs-orbital-entanglement-legend-title"
        >
          Mutual information
        </text>
        {Array.from({ length: numCbStops }, (_, k) => {
          const t = k / (numCbStops - 1);
          return (
            <rect
              key={`cb-mi-${k}`}
              x={cbX + (cbW * k) / numCbStops}
              y={cbY + cbH + 24}
              width={cbW / numCbStops + 0.5}
              height={cbH}
              fill={colormapEval(CHORD_CMAP, t)}
            />
          );
        })}
        <text
          x={cbX}
          y={cbY + cbH * 2 + 34}
          class="qs-orbital-entanglement-legend-value"
        >
          0
        </text>
        <text
          x={cbX + cbW}
          y={cbY + cbH * 2 + 34}
          class="qs-orbital-entanglement-legend-value qs-orbital-entanglement-legend-value-end"
        >
          {miMax.toFixed(2)}
        </text>
      </g>
    </svg>
  );
}
