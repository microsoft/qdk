// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// State visualization “prep” utilities.
// Converts raw amplitude maps into a small set of render-ready columns
// (normalization, thresholding, capping, and aggregating an "Others" bucket).
// Kept DOM-free so it can run on the main thread or inside a Web Worker.

import type {
  StateColumn,
  AmpComplex,
  AmpPolar,
  AmpLike,
  AmpMap,
} from "./stateViz.js";

export type PrepareStateVizOptions = {
  // normalize probabilities to unit mass (default true)
  normalize?: boolean;
  // Minimum probability (0..1) for a state to be shown as its own column.
  // States below this threshold will be aggregated into Others bucket.
  // Default: 0
  minProbThreshold?: number;
  // Maximum number of columns to render, including Others if present.
  // Default: 16
  maxColumns?: number;
};

const DEFAULT_MAX_COLUMNS = 16;
const MIN_PROB_EPSILON = 1e-12;

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

function normalizeColumns(columns: StateColumn[]): StateColumn[] {
  const sum = columns.reduce((acc, c) => acc + (c.prob ?? 0), 0);
  if (sum <= 0) return columns;
  return columns.map((c) => ({ ...c, prob: (c.prob ?? 0) / sum }));
}

function splitByThreshold(
  columns: StateColumn[],
  minProb: number,
): { significant: StateColumn[]; small: StateColumn[] } {
  const minThresh = Math.max(0, Math.min(1, minProb));
  const significant: StateColumn[] = [];
  const small: StateColumn[] = [];
  for (const c of columns) {
    if ((c.prob ?? 0) >= minThresh) significant.push(c);
    else small.push(c);
  }
  return { significant, small };
}

function capAndAggregateColumns(
  significant: StateColumn[],
  small: StateColumn[],
  maxColumns: number,
): StateColumn[] {
  const needsOthers = small.length > 0 || significant.length > maxColumns;
  const capacity = needsOthers
    ? Math.max(0, maxColumns - 1)
    : Math.max(1, maxColumns);

  const topColumns = significant.slice(0, capacity);
  const dropped = significant.slice(capacity);
  const othersList = [...small, ...dropped];

  const columns = [...topColumns];
  if (othersList.length > 0) {
    const othersProb = othersList.reduce((s, r) => s + (r.prob ?? 0), 0);
    const othersCount = othersList.length;
    columns.push({
      label: "__OTHERS__",
      prob: othersProb,
      phase: 0,
      isOthers: true,
      othersCount,
    });
  }
  return columns;
}

function sortColumnsByLabel(columns: StateColumn[]): StateColumn[] {
  const numericRegex = /^[+-]?\d+(?:\.\d+)?$/;
  const asNumber = (s: string) => (numericRegex.test(s) ? parseFloat(s) : NaN);
  const isNumeric = (s: string) => numericRegex.test(s);
  const labelCmp = (a: string, b: string) => a.localeCompare(b);
  const labelOrderCmp = (a: StateColumn, b: StateColumn) => {
    if (a.isOthers === true) return 1;
    if (b.isOthers === true) return -1;
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
  return columns.slice().sort(labelOrderCmp);
}

export function prepareStateVizColumnsFromAmpMap(
  ampMap: AmpMap,
  opts: PrepareStateVizOptions = {},
): StateColumn[] {
  const entries = Object.entries(ampMap ?? {});
  if (entries.length === 0) return [];

  const raw = entries.map(([label, a]) => {
    const { prob, phase } = toPolar(a);
    return {
      label,
      prob: prob < MIN_PROB_EPSILON ? 0 : prob,
      phase,
    } as StateColumn;
  });

  // Normalize probabilities
  const doNormalize = opts.normalize ?? true;
  const normalized = doNormalize ? normalizeColumns(raw) : raw;

  // Split columns based on threshold value
  const { significant, small } =
    typeof opts.minProbThreshold === "number"
      ? splitByThreshold(normalized, opts.minProbThreshold)
      : { significant: normalized, small: [] };

  // Sort by probability and cap to max column numbers, aggregating rest into "Others"
  significant.sort((a, b) => (b.prob ?? 0) - (a.prob ?? 0));
  const maxColumns =
    typeof opts.maxColumns === "number" && isFinite(opts.maxColumns)
      ? Math.max(1, Math.floor(opts.maxColumns))
      : DEFAULT_MAX_COLUMNS;
  const capped = capAndAggregateColumns(significant, small, maxColumns);
  // Sort by top columns by label, with "Others" last
  return sortColumnsByLabel(capped);
}
