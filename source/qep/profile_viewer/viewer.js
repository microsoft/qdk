// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

"use strict";

// Fixed, deliberately varied colors keep instruction names stable across files.
const INSTRUCTION_NAMES = [
  "ccx",
  "cx",
  "cy",
  "cz",
  "rx",
  "rxx",
  "ry",
  "ryy",
  "rz",
  "rzz",
  "h",
  "s",
  "sadj",
  "sx",
  "t",
  "tadj",
  "x",
  "y",
  "z",
  "swap",
  "reset",
  "mz_store",
  "mz_store_reset",
  "read_result",
  "mz",
  "mz_reset",
  "inject_t",
  "inject_tadj",
  "round",
];
const PALETTE = [
  "#d45b75",
  "#4196d5",
  "#b875d6",
  "#d69a39",
  "#4dad92",
  "#d77c4c",
  "#7a8cdf",
  "#a0a83f",
  "#d56fa4",
  "#45a8b9",
  "#9569cc",
  "#c8954b",
  "#5b9d65",
  "#c66050",
  "#6384b8",
  "#a87a54",
  "#ba6bc1",
  "#439d9e",
  "#a297cf",
  "#b69d40",
  "#748f47",
  "#bf6f89",
  "#628e98",
  "#9b7fb4",
  "#b48664",
  "#6684d1",
  "#a85c48",
  "#538a72",
  "#967246",
  "#6d9fac",
  "#bd837e",
  "#a88bce",
];

/** Rejects malformed exports before replacing the currently displayed profile. */
function validateProfile(profile) {
  /** Reports a field-specific validation failure. */
  function requireValue(condition, message) {
    if (!condition) throw new Error(message);
  }
  /** Recognizes nonnegative, finite numbers (including fractional rates). */
  function nonnegative(value) {
    return typeof value === "number" && Number.isFinite(value) && value >= 0;
  }
  /** Recognizes safe integer IDs and counts. */
  function count(value) {
    return Number.isSafeInteger(value) && value >= 0;
  }
  /** Recognizes plain JSON objects, excluding arrays and null. */
  function object(value) {
    return value !== null && typeof value === "object" && !Array.isArray(value);
  }

  requireValue(object(profile), "Expected a ProfilerResult JSON object.");
  requireValue(object(profile.totals), "Missing totals.");
  for (const key of [
    "logical_qubits",
    "logical_compute_qubits",
    "physical_qubits",
    "total_operations_count",
  ]) {
    requireValue(
      count(profile.totals[key]),
      `totals.${key} must be a nonnegative safe integer.`,
    );
  }
  requireValue(
    profile.totals.logical_compute_qubits <= profile.totals.logical_qubits,
    "totals.logical_compute_qubits exceeds totals.logical_qubits.",
  );
  requireValue(
    nonnegative(profile.totals.total_runtime_sec),
    "Invalid total runtime.",
  );
  for (const key of ["calls_per_operation", "runtime_sec_per_operation"]) {
    const values = profile.totals[key];
    requireValue(object(values), `Missing totals.${key}.`);
    for (const [name, value] of Object.entries(values)) {
      requireValue(
        key === "calls_per_operation" ? count(value) : nonnegative(value),
        `Invalid ${key} value for ${name}.`,
      );
    }
  }
  requireValue(
    object(profile.profiles) && Array.isArray(profile.profiles.profiles),
    "Missing time series.",
  );
  const window = profile.profiles.aggregation_window_sec;
  requireValue(
    window === null || (nonnegative(window) && window > 0),
    "Invalid aggregation window.",
  );
  for (const metric of profile.profiles.profiles) {
    requireValue(
      object(metric) &&
        typeof metric.metric_name === "string" &&
        typeof metric.metric_units === "string" &&
        Array.isArray(metric.series),
      "Each time series needs a name, units, and samples.",
    );
    let previous = -Infinity;
    for (const point of metric.series) {
      requireValue(
        Array.isArray(point) &&
          point.length === 2 &&
          nonnegative(point[0]) &&
          nonnegative(point[1]) &&
          point[0] >= previous,
        `Invalid or unsorted sample in ${metric.metric_name}.`,
      );
      previous = point[0];
    }
  }
  requireValue(
    Array.isArray(profile.infos) &&
      profile.infos.every((info) => typeof info === "string"),
    "Invalid informational messages.",
  );
  requireValue(
    profile.trace === null ||
      (object(profile.trace) && Array.isArray(profile.trace.instructions)),
    "Invalid execution trace.",
  );
  if (profile.trace !== null) {
    for (const entry of profile.trace.instructions) {
      requireValue(
        object(entry) &&
          count(entry.start_time_nanos) &&
          count(entry.end_time_nanos) &&
          entry.end_time_nanos >= entry.start_time_nanos &&
          object(entry.instruction) &&
          typeof entry.instruction.instruction_name === "string" &&
          Array.isArray(entry.instruction.parameters),
        "Invalid trace instruction or interval.",
      );
      for (const parameter of entry.instruction.parameters) {
        requireValue(
          object(parameter) && Object.keys(parameter).length === 1,
          "Invalid trace parameter.",
        );
        const [kind, value] = Object.entries(parameter)[0];
        requireValue(
          kind === "Float"
            ? typeof value === "number" && Number.isFinite(value)
            : ["QubitId", "FactoryId", "ResultId"].includes(kind) &&
                count(value),
          `Invalid ${kind} parameter.`,
        );
        if (kind === "QubitId") {
          requireValue(
            value < profile.totals.logical_compute_qubits,
            "Trace qubit ID exceeds totals.logical_compute_qubits.",
          );
        }
      }
    }
  }
  return profile;
}

/** Finds an insertion boundary, optionally placing the target after equal values. */
function timeBound(values, target, afterEqual) {
  let low = 0;
  let high = values.length;
  while (low < high) {
    const middle = Math.floor((low + high) / 2);
    if (values[middle] < target || (afterEqual && values[middle] === target))
      low = middle + 1;
    else high = middle;
  }
  return low;
}

/** Lazily indexes lanes in bounded chunks; no work occurs until the iterator advances. */
function* buildTraceIndex(profile) {
  const lanes = new Map();
  let extent = profile.totals.total_runtime_sec * 1e9;
  let unplaced = 0;
  let work = 0;
  for (const entry of profile.trace.instructions) {
    extent = Math.max(extent, entry.end_time_nanos);
    let placed = false;
    const seen = new Set();
    for (const parameter of entry.instruction.parameters) {
      const kind =
        "QubitId" in parameter
          ? "QubitId"
          : "FactoryId" in parameter
            ? "FactoryId"
            : null;
      if (!kind) continue;
      const id = parameter[kind];
      const key = `${kind}:${id}`;
      if (seen.has(key)) continue;
      seen.add(key);
      if (!lanes.has(key)) lanes.set(key, { kind, id, entries: [] });
      lanes.get(key).entries.push(entry);
      placed = true;
    }
    if (!placed) unplaced += 1;
    if (++work % 10000 === 0) yield;
  }
  const rows = [...lanes.values()].sort((a, b) => {
    if (a.kind !== b.kind) return a.kind === "QubitId" ? -1 : 1;
    return a.id - b.id;
  });
  // Empty logical lanes matter too, but enormous sparse traces must not allocate
  // an array for every possible ID. The renderer synthesizes empty qubit rows.
  const qubits = new Map();
  const factories = [];
  for (const row of rows) {
    let sorted = true;
    for (let index = 1; index < row.entries.length; index += 1) {
      if (
        row.entries[index - 1].start_time_nanos >
        row.entries[index].start_time_nanos
      )
        sorted = false;
      if (++work % 10000 === 0) yield;
    }
    if (!sorted)
      row.entries.sort((a, b) => a.start_time_nanos - b.start_time_nanos);
    row.starts = new Float64Array(row.entries.length);
    row.maxEnds = new Float64Array(row.entries.length);
    let maximum = 0;
    for (let index = 0; index < row.entries.length; index += 1) {
      const entry = row.entries[index];
      row.starts[index] = entry.start_time_nanos;
      maximum = Math.max(maximum, entry.end_time_nanos);
      row.maxEnds[index] = maximum;
      if (++work % 10000 === 0) yield;
    }
    if (row.kind === "QubitId") qubits.set(row.id, row);
    else factories.push(row);
  }
  return { qubits, factories, extent: Math.max(1, extent), unplaced };
}

/** Returns intervals overlapping a visible time range, including zero-duration events. */
function visibleEntries(row, start, end) {
  if (!row) return [];
  const first = timeBound(row.maxEnds, start, false);
  const last = timeBound(row.starts, end, true);
  const visible = [];
  for (let index = first; index < last; index += 1) {
    const entry = row.entries[index];
    if (entry.end_time_nanos >= start) visible.push(entry);
  }
  return visible;
}

/** Preserves peaks while limiting a chart path to at most four points per pixel column. */
function decimateSeries(points, duration, pixels) {
  if (points.length <= pixels * 4) return points;
  const result = [];
  let bucket = -1;
  let group = [];
  /** Emits the first, extrema, and last sample in original time order. */
  function flush() {
    if (!group.length) return;
    let minimum = 0;
    let maximum = 0;
    for (let index = 1; index < group.length; index += 1) {
      if (group[index][1] < group[minimum][1]) minimum = index;
      if (group[index][1] > group[maximum][1]) maximum = index;
    }
    for (const index of [
      ...new Set([0, minimum, maximum, group.length - 1]),
    ].sort((a, b) => a - b)) {
      result.push(group[index]);
    }
    group = [];
  }
  for (const point of points) {
    const next = Math.min(
      pixels - 1,
      Math.floor((point[0] / duration) * pixels),
    );
    if (next !== bucket) flush();
    bucket = next;
    group.push(point);
  }
  flush();
  return result;
}

/** Formats an axis value without losing small sub-second times. */
function formatNumber(value) {
  if (value === 0) return "0";
  if (Math.abs(value) < 0.001 || Math.abs(value) >= 1e7)
    return value.toExponential(2);
  return new Intl.NumberFormat("en", { maximumSignificantDigits: 6 }).format(
    value,
  );
}

// The same pure helpers are testable in Node; the page itself uses a classic
// script, not an ES module, so opening index.html via file:// works offline.
if (typeof module !== "undefined" && module.exports) {
  module.exports = {
    validateProfile,
    buildTraceIndex,
    visibleEntries,
    decimateSeries,
  };
}
if (typeof document !== "undefined") initializeViewer();

/** Wires file loading, rate charts, and a trace renderer with no hidden background loop. */
function initializeViewer() {
  /** Resolves an element owned by this page. */
  const element = (id) => document.getElementById(id);
  const status = element("status");
  const tooltip = element("tooltip");
  const ratesCanvas = element("rates-canvas");
  const traceCanvas = element("trace-canvas");
  const traceScroll = element("trace-scroll");
  const traceSection = element("trace-section");
  const selected = new Set();
  let profile = null;
  let loadId = 0;
  let chartFrame = 0;
  let traceFrame = 0;
  let indexFrame = 0;
  let indexBuilder = null;
  let traceIndex = null;
  let rendered = [];
  let viewDuration = 1;
  let viewStart = 0;
  let traceGeometry = null;
  const GUTTER = 125;
  const RULER = 35;
  const ROW_HEIGHT = 32;
  const MAX_SCROLL_WIDTH = 8000000;

  /** Shows explicit loading or failure feedback without interpreting file content as HTML. */
  function setStatus(message, error = false) {
    status.textContent = message;
    status.dataset.error = String(error);
  }

  /** Positions a shared plain-text tooltip near the pointer. */
  function showTooltip(text, event) {
    tooltip.textContent = text;
    tooltip.hidden = false;
    const bounds = tooltip.getBoundingClientRect();
    tooltip.style.left = `${Math.max(8, Math.min(event.clientX + 14, innerWidth - bounds.width - 10))}px`;
    tooltip.style.top = `${Math.max(8, Math.min(event.clientY + 14, innerHeight - bounds.height - 10))}px`;
  }

  /** Clears hover state when moving or hiding a visualization. */
  function hideTooltip() {
    tooltip.hidden = true;
  }

  /** Loads a browser File or relative URL with last-selection-wins protection. */
  async function loadProfile(source) {
    if (!source) return;
    const fromUrl = typeof source === "string";
    const name = fromUrl ? source : source.name;
    const request = ++loadId;
    setStatus(
      fromUrl
        ? `Loading ${name}...`
        : `Loading ${name} (${formatNumber(source.size / 1024)} KiB)...`,
    );
    try {
      let contents;
      if (fromUrl) {
        const response = await fetch(source);
        if (!response.ok)
          throw new Error(`HTTP ${response.status} ${response.statusText}`);
        contents = await response.text();
      } else {
        contents = await source.text();
      }
      if (request !== loadId) return;
      const next = validateProfile(JSON.parse(contents));
      stopTrace();
      profile = next;
      traceIndex = null;
      element("profile-content").hidden = false;
      element("profile-name").textContent = name;
      renderSummary();
      renderSeriesSelector();
      traceSection.hidden = profile.trace === null;
      if (profile.trace !== null) {
        if (profile.trace.instructions.length === 0) {
          element("trace-message").textContent = "The recorded trace is empty.";
        } else {
          startTrace();
        }
      }
      setStatus(`Loaded ${name}.`);
      queueChart();
    } catch (error) {
      if (request !== loadId) return;
      setStatus(
        `Could not load ${name}: ${error.message}${profile ? " The previous profile remains displayed." : ""}`,
        true,
      );
    }
  }

  /** Builds the summary cards and every exported per-operation total. */
  function renderSummary() {
    const totals = profile.totals;
    const cards = [
      ["Logical qubits", totals.logical_qubits],
      ["Logical compute qubits", totals.logical_compute_qubits],
      ["Physical qubits (reported)", totals.physical_qubits],
      ["Runtime, s", totals.total_runtime_sec],
      ["Operation / event count", totals.total_operations_count],
    ];
    const summary = element("summary");
    summary.replaceChildren();
    for (const [label, value] of cards) {
      const card = document.createElement("div");
      card.className = "stat";
      const term = document.createElement("dt");
      term.textContent = label;
      const description = document.createElement("dd");
      description.textContent = value.toLocaleString("en", {
        maximumSignificantDigits: 16,
      });
      description.title = String(value);
      card.append(term, description);
      summary.append(card);
    }
    const infos = element("infos");
    infos.replaceChildren();
    for (const info of profile.infos) {
      const paragraph = document.createElement("p");
      paragraph.textContent = info;
      infos.append(paragraph);
    }
    const container = element("operation-totals");
    container.replaceChildren();
    const names = [
      ...new Set([
        ...Object.keys(totals.calls_per_operation),
        ...Object.keys(totals.runtime_sec_per_operation),
      ]),
    ].sort();
    if (!names.length) {
      container.textContent = "No per-operation totals recorded.";
      return;
    }
    const table = document.createElement("table");
    for (const values of [
      ["Instruction", "Calls", "Runtime, s"],
      ...names.map((name) => [
        name,
        Object.hasOwn(totals.calls_per_operation, name)
          ? String(totals.calls_per_operation[name])
          : "-",
        Object.hasOwn(totals.runtime_sec_per_operation, name)
          ? formatNumber(totals.runtime_sec_per_operation[name])
          : "-",
      ]),
    ]) {
      const row = document.createElement("tr");
      for (const value of values) {
        const cell = document.createElement(
          table.rows.length === 0 ? "th" : "td",
        );
        cell.textContent = value;
        row.append(cell);
      }
      table.append(row);
    }
    container.append(table);
  }

  /** Assigns distinct fixed colors to known instructions and deterministic colors to other names. */
  function instructionColor(name) {
    const known = INSTRUCTION_NAMES.indexOf(name);
    if (known !== -1) return PALETTE[known];
    let hash = 0;
    for (const character of name)
      hash = (hash * 31 + character.charCodeAt(0)) | 0;
    return `hsl(${((hash % 360) + 360) % 360} 45% 45%)`;
  }

  /** Creates accessible series toggles, showing each series' own unit. */
  function renderSeriesSelector() {
    const list = element("series-list");
    list.replaceChildren();
    selected.clear();
    profile.profiles.profiles.forEach((metric, index) => {
      selected.add(index);
      const label = document.createElement("label");
      label.className = "series-option";
      const checkbox = document.createElement("input");
      checkbox.type = "checkbox";
      checkbox.checked = true;
      checkbox.dataset.index = String(index);
      checkbox.addEventListener("change", () => {
        if (checkbox.checked) selected.add(index);
        else selected.delete(index);
        queueChart();
      });
      const swatch = document.createElement("span");
      swatch.className = "swatch";
      swatch.style.background = PALETTE[index % PALETTE.length];
      const text = document.createElement("span");
      text.textContent = metric.metric_name;
      const units = document.createElement("small");
      units.textContent = metric.metric_units;
      text.append(units);
      label.append(checkbox, swatch, text);
      list.append(label);
    });
    element("series-empty").hidden = profile.profiles.profiles.length !== 0;
    const window = profile.profiles.aggregation_window_sec;
    element("window-note").textContent =
      window === null
        ? "No fixed aggregation window"
        : `Aggregation window: ${formatNumber(window)} s`;
  }

  /** Scales a viewport-sized backing store for sharp high-DPI canvas rendering. */
  function prepareCanvas(canvas, width, height) {
    const ratio = devicePixelRatio || 1;
    const pixelWidth = Math.max(1, Math.round(width * ratio));
    const pixelHeight = Math.max(1, Math.round(height * ratio));
    if (canvas.width !== pixelWidth) canvas.width = pixelWidth;
    if (canvas.height !== pixelHeight) canvas.height = pixelHeight;
    canvas.style.width = `${width}px`;
    canvas.style.height = `${height}px`;
    const context = canvas.getContext("2d");
    context.setTransform(ratio, 0, 0, ratio, 0, 0);
    context.clearRect(0, 0, width, height);
    context.font = "12px system-ui";
    return context;
  }

  /** Covers the runtime and all sample timestamps, including zero-runtime profiles. */
  function chartDuration() {
    let end = profile.totals.total_runtime_sec;
    for (const metric of profile.profiles.profiles) {
      if (metric.series.length)
        end = Math.max(end, metric.series[metric.series.length - 1][0]);
    }
    return end || profile.profiles.aggregation_window_sec || 1;
  }

  /** Schedules a single chart redraw rather than running a continuous animation loop. */
  function queueChart() {
    if (!chartFrame)
      chartFrame = requestAnimationFrame(() => {
        chartFrame = 0;
        if (profile) drawChart();
      });
  }

  /** Draws step-like rate series, numeric axes, and a time-only x-axis label. */
  function drawChart() {
    const width = element("chart-wrap").clientWidth;
    const height = element("chart-wrap").clientHeight;
    if (!width || !height) return;
    const context = prepareCanvas(ratesCanvas, width, height);
    const left = Math.min(78, width / 4);
    const top = 20;
    const plotWidth = Math.max(1, width - left - 20);
    const plotHeight = height - top - 58;
    const duration = chartDuration();
    let maximum = 0;
    for (const index of selected) {
      for (const point of profile.profiles.profiles[index].series)
        maximum = Math.max(maximum, point[1]);
    }
    maximum = maximum ? maximum * 1.08 : 1;
    context.textBaseline = "middle";
    for (let tick = 0; tick <= 5; tick += 1) {
      const y = top + plotHeight * (1 - tick / 5);
      context.strokeStyle = "#e1e7ef";
      context.beginPath();
      context.moveTo(left, y);
      context.lineTo(left + plotWidth, y);
      context.stroke();
      context.fillStyle = "#53647a";
      context.textAlign = "right";
      context.fillText(formatNumber((maximum * tick) / 5), left - 10, y);
      const x = left + (plotWidth * tick) / 5;
      context.textAlign = "center";
      context.fillText(
        formatNumber((duration * tick) / 5),
        x,
        top + plotHeight + 20,
      );
    }
    context.fillText("time, s", left + plotWidth / 2, height - 10);
    context.save();
    context.beginPath();
    context.rect(left, top, plotWidth, plotHeight);
    context.clip();
    for (const index of selected) {
      const series = profile.profiles.profiles[index].series;
      const points = decimateSeries(
        series,
        duration,
        Math.max(1, Math.floor(plotWidth)),
      );
      if (!points.length) continue;
      context.strokeStyle = PALETTE[index % PALETTE.length];
      context.lineWidth = 1.5;
      context.beginPath();
      let previousY = 0;
      points.forEach(([time, value], pointIndex) => {
        const x = left + (time / duration) * plotWidth;
        const y = top + plotHeight * (1 - value / maximum);
        if (pointIndex === 0) context.moveTo(x, y);
        else {
          context.lineTo(x, previousY);
          context.lineTo(x, y);
        }
        previousY = y;
      });
      const last = points[points.length - 1];
      const window = profile.profiles.aggregation_window_sec;
      const end = Math.min(
        duration,
        window === null ? last[0] : last[0] + window,
      );
      context.lineTo(left + (end / duration) * plotWidth, previousY);
      context.stroke();
      if (points.length === 1 && end === last[0]) {
        context.fillStyle = PALETTE[index % PALETTE.length];
        context.fillRect(
          left + (last[0] / duration) * plotWidth - 2,
          previousY - 2,
          4,
          4,
        );
      }
    }
    context.restore();
    if (selected.size === 0 || profile.profiles.profiles.length === 0) {
      context.fillStyle = "#53647a";
      context.textAlign = "center";
      context.fillText(
        "Select time series to display",
        left + plotWidth / 2,
        top + plotHeight / 2,
      );
    }
  }

  /** Cancels trace work and releases its backing canvas before replacing a profile. */
  function stopTrace() {
    cancelAnimationFrame(traceFrame);
    cancelAnimationFrame(indexFrame);
    traceFrame = 0;
    indexFrame = 0;
    indexBuilder = null;
    traceObserver.disconnect();
    rendered = [];
    traceGeometry = null;
    traceCanvas.width = 0;
    traceCanvas.height = 0;
    element("trace-content").hidden = true;
    hideTooltip();
  }

  /** Builds the trace index only while visible and yields between chunks to stay responsive. */
  function startTrace() {
    if (!profile?.trace || traceSection.hidden) return;
    element("trace-content").hidden = false;
    if (traceIndex) {
      traceObserver.observe(traceScroll);
      configureTrace(viewStart);
      return;
    }
    element("trace-message").textContent = "Indexing trace lanes...";
    indexBuilder = buildTraceIndex(profile);
    /** Advances at most one index chunk per animation frame. */
    function advanceIndex() {
      indexFrame = 0;
      if (traceSection.hidden || !indexBuilder) return;
      const step = indexBuilder.next();
      if (!step.done) {
        indexFrame = requestAnimationFrame(advanceIndex);
        return;
      }
      traceIndex = step.value;
      indexBuilder = null;
      viewDuration = traceIndex.extent;
      viewStart = 0;
      const omitted = traceIndex.unplaced;
      element("trace-message").textContent =
        `${profile.totals.logical_compute_qubits} logical compute qubit lanes; ${traceIndex.factories.length} factory lanes.` +
        (omitted
          ? ` ${omitted} instructions without qubit/factory operands (such as read_result) have no lane.`
          : "") +
        " Factory rounds may extend beyond the algorithm's reported runtime.";
      traceObserver.observe(traceScroll);
      configureTrace(0);
    }
    indexFrame = requestAnimationFrame(advanceIndex);
  }

  /** Maps an arbitrary zoom range to a bounded native scrollbar, avoiding browser size limits. */
  function configureTrace(start) {
    if (traceSection.hidden || !traceIndex) return;
    const width = traceScroll.clientWidth;
    const height = traceScroll.clientHeight;
    if (!width || !height) return;
    const plotWidth = Math.max(1, width - GUTTER);
    viewDuration = Math.max(1, Math.min(traceIndex.extent, viewDuration));
    const spacer = element("trace-spacer");
    spacer.style.width = `${Math.min(MAX_SCROLL_WIDTH, Math.max(width, (traceIndex.extent / viewDuration) * plotWidth + GUTTER))}px`;
    spacer.style.height = `${Math.max(height, RULER + (profile.totals.logical_compute_qubits + traceIndex.factories.length) * ROW_HEIGHT)}px`;
    viewStart = Math.max(0, Math.min(start, traceIndex.extent - viewDuration));
    const maximumScroll = traceScroll.scrollWidth - traceScroll.clientWidth;
    traceScroll.scrollLeft =
      traceIndex.extent === viewDuration
        ? 0
        : (maximumScroll * viewStart) / (traceIndex.extent - viewDuration);
    queueTrace();
  }

  /** Coalesces scroll and zoom events and does no work for a hidden trace. */
  function queueTrace() {
    if (traceSection.hidden || !traceIndex || traceFrame) return;
    traceFrame = requestAnimationFrame(() => {
      traceFrame = 0;
      if (!traceSection.hidden) drawTrace();
    });
  }

  /** Draws only visible lanes and time-overlapping instructions on a viewport-sized canvas. */
  function drawTrace() {
    const width = traceScroll.clientWidth;
    const height = traceScroll.clientHeight;
    const context = prepareCanvas(traceCanvas, width, height);
    const plotWidth = Math.max(1, width - GUTTER);
    const maximumScroll = traceScroll.scrollWidth - width;
    viewStart =
      maximumScroll > 0
        ? (traceScroll.scrollLeft / maximumScroll) *
          (traceIndex.extent - viewDuration)
        : 0;
    const end = viewStart + viewDuration;
    const firstRow = Math.max(
      0,
      Math.floor(traceScroll.scrollTop / ROW_HEIGHT),
    );
    const lastRow = Math.min(
      profile.totals.logical_compute_qubits + traceIndex.factories.length,
      Math.ceil((traceScroll.scrollTop + height - RULER) / ROW_HEIGHT),
    );
    rendered = [];
    context.fillStyle = "#ffffff";
    context.fillRect(0, 0, width, height);
    context.save();
    context.beginPath();
    context.rect(GUTTER, RULER, plotWidth, height - RULER);
    context.clip();
    for (let rowIndex = firstRow; rowIndex < lastRow; rowIndex += 1) {
      const y = RULER + rowIndex * ROW_HEIGHT - traceScroll.scrollTop;
      const row =
        rowIndex < profile.totals.logical_compute_qubits
          ? traceIndex.qubits.get(rowIndex)
          : traceIndex.factories[
              rowIndex - profile.totals.logical_compute_qubits
            ];
      context.strokeStyle = "#d4dce8";
      context.beginPath();
      context.moveTo(GUTTER, y + ROW_HEIGHT / 2);
      context.lineTo(width, y + ROW_HEIGHT / 2);
      context.stroke();
      for (const entry of visibleEntries(row, viewStart, end)) {
        const x =
          GUTTER +
          ((entry.start_time_nanos - viewStart) / viewDuration) * plotWidth;
        const right =
          GUTTER +
          ((entry.end_time_nanos - viewStart) / viewDuration) * plotWidth;
        const clippedX = Math.max(GUTTER, x);
        const rectangleWidth = Math.max(
          1,
          Math.min(width, Math.max(x + 1, right)) - clippedX,
        );
        context.fillStyle = instructionColor(
          entry.instruction.instruction_name,
        );
        context.fillRect(clippedX, y + 5, rectangleWidth, ROW_HEIGHT - 10);
        const label = entry.instruction.instruction_name;
        if (rectangleWidth > context.measureText(label).width + 10) {
          context.fillStyle = "#ffffff";
          context.textAlign = "left";
          context.textBaseline = "middle";
          context.fillText(label, clippedX + 5, y + ROW_HEIGHT / 2);
        }
        rendered.push({
          entry,
          x: clippedX,
          y: y + 5,
          width: rectangleWidth,
          height: ROW_HEIGHT - 10,
        });
      }
    }
    context.restore();
    context.fillStyle = "#f2f5f9";
    context.fillRect(0, 0, GUTTER, height);
    context.fillRect(0, 0, width, RULER);
    context.fillStyle = "#334155";
    context.textAlign = "left";
    context.textBaseline = "middle";
    for (let rowIndex = firstRow; rowIndex < lastRow; rowIndex += 1) {
      const label =
        rowIndex < profile.totals.logical_compute_qubits
          ? `Qubit ${rowIndex}`
          : `Factory ${traceIndex.factories[rowIndex - profile.totals.logical_compute_qubits].id}`;
      context.fillText(
        label,
        10,
        RULER + rowIndex * ROW_HEIGHT - traceScroll.scrollTop + ROW_HEIGHT / 2,
      );
    }
    context.fillText("time, s", 10, RULER / 2);
    for (let tick = 0; tick <= 5; tick += 1) {
      const x = GUTTER + (plotWidth * tick) / 5;
      context.textAlign = tick === 0 ? "left" : tick === 5 ? "right" : "center";
      context.fillText(
        formatNumber((viewStart + (viewDuration * tick) / 5) / 1e9),
        x,
        RULER / 2,
      );
    }
    traceGeometry = { width, height, plotWidth };
    element("trace-scale").textContent =
      `${formatNumber(viewDuration / 1e9)} s visible`;
    element("trace-stats").textContent =
      `${rendered.length.toLocaleString()} visible instruction rectangles; ` +
      (lastRow > firstRow
        ? `lanes ${firstRow + 1}-${lastRow}.`
        : "no qubit or factory lanes.");
  }

  /** Zooms around a fractional point of the current viewport, preserving its timestamp. */
  function zoomTrace(factor, anchor = 0.5) {
    if (!traceIndex || traceSection.hidden) return;
    const time = viewStart + viewDuration * anchor;
    viewDuration = Math.max(
      1,
      Math.min(traceIndex.extent, viewDuration * factor),
    );
    configureTrace(time - viewDuration * anchor);
    hideTooltip();
  }

  element("file-input").addEventListener("change", (event) => {
    loadProfile(event.target.files[0]);
    event.target.value = "";
  });
  for (const [id, checked] of [
    ["series-all", true],
    ["series-none", false],
  ]) {
    element(id).addEventListener("click", () => {
      if (!profile) return;
      selected.clear();
      for (const checkbox of element("series-list").querySelectorAll("input")) {
        checkbox.checked = checked;
        if (checked) selected.add(Number(checkbox.dataset.index));
      }
      queueChart();
    });
  }
  element("zoom-in").addEventListener("click", () => zoomTrace(0.5));
  element("zoom-out").addEventListener("click", () => zoomTrace(2));
  element("zoom-fit").addEventListener("click", () => {
    if (!traceIndex) return;
    viewDuration = traceIndex.extent;
    configureTrace(0);
  });
  traceScroll.addEventListener(
    "scroll",
    () => {
      if (!traceSection.hidden) {
        hideTooltip();
        queueTrace();
      }
    },
    { passive: true },
  );
  traceScroll.addEventListener(
    "wheel",
    (event) => {
      if (
        traceSection.hidden ||
        (!event.ctrlKey && !event.metaKey) ||
        !traceGeometry
      )
        return;
      event.preventDefault();
      const bounds = traceCanvas.getBoundingClientRect();
      const anchor = Math.max(
        0,
        Math.min(
          1,
          (event.clientX - bounds.left - GUTTER) / traceGeometry.plotWidth,
        ),
      );
      zoomTrace(event.deltaY > 0 ? 1.25 : 0.8, anchor);
    },
    { passive: false },
  );
  traceCanvas.addEventListener("mousemove", (event) => {
    if (traceSection.hidden || !traceGeometry) return;
    const bounds = traceCanvas.getBoundingClientRect();
    const x = event.clientX - bounds.left;
    const y = event.clientY - bounds.top;
    if (x < GUTTER || y < RULER) return hideTooltip();
    const hit = rendered.findLast(
      (rectangle) =>
        x >= rectangle.x &&
        x <= rectangle.x + rectangle.width &&
        y >= rectangle.y &&
        y <= rectangle.y + rectangle.height,
    );
    if (!hit) return hideTooltip();
    const entry = hit.entry;
    const operands = entry.instruction.parameters
      .map((parameter) => {
        const [kind, value] = Object.entries(parameter)[0];
        return `${kind}: ${value}`;
      })
      .join(", ");
    showTooltip(
      `${entry.instruction.instruction_name}\n${operands}\nStart: ${formatNumber(entry.start_time_nanos / 1e9)} s\nEnd: ${formatNumber(entry.end_time_nanos / 1e9)} s\nDuration: ${formatNumber(entry.end_time_nanos - entry.start_time_nanos)} ns`,
      event,
    );
  });
  ratesCanvas.addEventListener("mousemove", (event) => {
    if (!profile) return;
    const bounds = ratesCanvas.getBoundingClientRect();
    const left = Math.min(78, bounds.width / 4);
    const fraction =
      (event.clientX - bounds.left - left) /
      Math.max(1, bounds.width - left - 20);
    if (fraction < 0 || fraction > 1) return hideTooltip();
    const time = fraction * chartDuration();
    const lines = [`Time: ${formatNumber(time)} s`];
    for (const index of selected) {
      const metric = profile.profiles.profiles[index];
      let low = 0;
      let high = metric.series.length;
      while (low < high) {
        const middle = Math.floor((low + high) / 2);
        if (metric.series[middle][0] <= time) low = middle + 1;
        else high = middle;
      }
      const point = metric.series[low - 1];
      const window = profile.profiles.aggregation_window_sec;
      if (point && (window === null || time < point[0] + window)) {
        lines.push(
          `${metric.metric_name}: ${formatNumber(point[1])} ${metric.metric_units}`,
        );
      }
    }
    showTooltip(lines.join("\n"), event);
  });
  ratesCanvas.addEventListener("mouseleave", hideTooltip);
  traceCanvas.addEventListener("mouseleave", hideTooltip);
  const chartObserver = new ResizeObserver(queueChart);
  chartObserver.observe(element("chart-wrap"));
  const traceObserver = new ResizeObserver(() => {
    if (!traceSection.hidden && traceIndex) configureTrace(viewStart);
  });

  const profileName = new URLSearchParams(location.search).get("profile_name");
  if (profileName !== null) {
    if (!/^[A-Za-z0-9_-]+$/.test(profileName)) {
      setStatus(
        "Invalid profile_name: use only letters, digits, underscores, or hyphens.",
        true,
      );
    } else if (location.protocol === "file:") {
      setStatus(
        "Automatic profile loading requires HTTP or HTTPS. Use Open JSON file instead.",
        true,
      );
    } else {
      loadProfile(`profiles/${profileName}.json`);
    }
  }
}
