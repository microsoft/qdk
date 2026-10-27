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

/** Chooses 3-10 evenly spaced ticks, preferring fewer decimal places and about six labels. */
function axisTicks(minimum, maximum) {
  if (
    !Number.isFinite(minimum) ||
    !Number.isFinite(maximum) ||
    maximum <= minimum
  )
    throw new RangeError("Axis bounds must be finite and increasing.");
  const magnitude = 10 ** Math.floor(Math.log10((maximum - minimum) / 5));
  let best = null;
  for (const factor of [1, 2, 5, 10]) {
    const step = magnitude * factor;
    const decimals = Math.max(0, -Math.floor(Math.log10(step)));
    const first = Math.ceil(minimum / step - 1e-10);
    const last = Math.floor(maximum / step + 1e-10);
    const count = last - first + 1;
    if (count < 3 || count > 10) continue;
    const ticks = Array.from({ length: count }, (_, index) => {
      const value = (first + index) * step;
      return Number(
        decimals <= 100 ? value.toFixed(decimals) : value.toPrecision(16),
      );
    }).filter((value) => value >= minimum && value <= maximum);
    if (ticks.length < 3 || new Set(ticks).size !== ticks.length) continue;
    const distance = Math.abs(ticks.length - 6);
    if (
      !best ||
      decimals < best.decimals ||
      (decimals === best.decimals && distance < best.distance)
    ) {
      best = { ticks, step, decimals, distance };
    }
  }
  if (!best)
    throw new RangeError("Axis range is too small to place distinct ticks.");
  return { ticks: best.ticks, step: best.step };
}

/** Formats tick labels at the step's precision without unnecessary trailing zeros. */
function formatAxisTick(value, step) {
  if (value === 0) return "0";
  const exponent = Math.floor(Math.log10(step));
  const decimals = Math.max(0, -exponent);
  if (Math.abs(value) < 0.001 || Math.abs(value) >= 1e7 || decimals > 20) {
    const precision = Math.max(
      0,
      Math.floor(Math.log10(Math.abs(value))) - exponent,
    );
    return value.toExponential(Math.min(100, precision)).replace(/\.?0+e/, "e");
  }
  return new Intl.NumberFormat("en", {
    maximumFractionDigits: decimals,
  }).format(value);
}

// The same pure helpers are testable in Node; the page itself uses a classic
// script, not an ES module, so opening index.html via file:// works offline.
if (typeof module !== "undefined" && module.exports) {
  module.exports = {
    validateProfile,
    buildTraceIndex,
    visibleEntries,
    decimateSeries,
    axisTicks,
    formatAxisTick,
  };
}
if (typeof document !== "undefined") initializeViewer();

/** Wires file loading, rate charts, and a trace renderer with no hidden background loop. */
function initializeViewer() {
  /** Resolves an element owned by this page. */
  const element = (id) => document.getElementById(id);
  const status = element("status");
  const tooltip = element("tooltip");
  const traceCanvas = element("trace-canvas");
  const traceScroll = element("trace-scroll");
  const traceSection = element("trace-section");
  const charts = [];
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
      renderCharts();
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

  /** Builds one chart and checkbox list for each distinct units string. */
  function renderCharts() {
    chartObserver.disconnect();
    hideTooltip();
    charts.length = 0;
    const container = element("charts");
    container.replaceChildren();
    const groups = new Map();
    profile.profiles.profiles.forEach((metric, index) => {
      if (!groups.has(metric.metric_units)) groups.set(metric.metric_units, []);
      groups.get(metric.metric_units).push(index);
    });
    if (groups.size === 0) groups.set("", []);
    const window = profile.profiles.aggregation_window_sec;
    for (const [units, indices] of groups) {
      const group =
        element("chart-template").content.firstElementChild.cloneNode(true);
      group.querySelector(".chart-units").textContent = units || "Time series";
      group.querySelector(".window-note").textContent =
        window === null
          ? "No fixed aggregation window"
          : `Aggregation window: ${formatNumber(window)} s`;
      group.setAttribute(
        "aria-label",
        units ? `Time series in ${units}` : "Time series",
      );
      const chart = {
        canvas: group.querySelector(".rates-canvas"),
        wrap: group.querySelector(".chart-wrap"),
        list: group.querySelector(".series-list"),
        indices,
        selected: new Set(indices),
      };
      chart.canvas.setAttribute(
        "aria-label",
        units
          ? `Selected time series in ${units} over time`
          : "Selected time series over time",
      );
      renderSeriesSelector(chart);
      group.querySelector(".series-empty").hidden = indices.length !== 0;
      for (const [selector, checked] of [
        [".series-all", true],
        [".series-none", false],
      ]) {
        group.querySelector(selector).addEventListener("click", () => {
          chart.selected.clear();
          for (const checkbox of chart.list.querySelectorAll("input")) {
            checkbox.checked = checked;
            if (checked) chart.selected.add(Number(checkbox.dataset.index));
          }
          hideTooltip();
          queueChart();
        });
      }
      chart.canvas.addEventListener("mousemove", (event) =>
        showChartTooltip(chart, event),
      );
      chart.canvas.addEventListener("mouseleave", hideTooltip);
      container.append(group);
      charts.push(chart);
      chartObserver.observe(chart.wrap);
    }
  }

  /** Creates accessible series toggles for a single units group. */
  function renderSeriesSelector(chart) {
    const list = chart.list;
    list.replaceChildren();
    for (const index of chart.indices) {
      const metric = profile.profiles.profiles[index];
      const label = document.createElement("label");
      label.className = "series-option";
      const checkbox = document.createElement("input");
      checkbox.type = "checkbox";
      checkbox.checked = true;
      checkbox.dataset.index = String(index);
      checkbox.addEventListener("change", () => {
        if (checkbox.checked) chart.selected.add(index);
        else chart.selected.delete(index);
        hideTooltip();
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
    }
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
        if (profile) {
          for (const chart of charts) drawChart(chart);
        }
      });
  }

  /** Draws step-like rate series, numeric axes, and a time-only x-axis label. */
  function drawChart(chart) {
    const width = chart.wrap.clientWidth;
    const height = chart.wrap.clientHeight;
    if (!width || !height) return;
    const context = prepareCanvas(chart.canvas, width, height);
    const left = Math.min(78, width / 4);
    const top = 20;
    const plotWidth = Math.max(1, width - left - 20);
    const plotHeight = height - top - 58;
    const duration = chartDuration();
    let maximum = 0;
    for (const index of chart.selected) {
      for (const point of profile.profiles.profiles[index].series)
        maximum = Math.max(maximum, point[1]);
    }
    maximum = maximum ? maximum * 1.08 : 1;
    context.textBaseline = "middle";
    const yAxis = axisTicks(0, maximum);
    for (const tick of yAxis.ticks) {
      const y = top + plotHeight * (1 - tick / maximum);
      context.strokeStyle = "#e1e7ef";
      context.beginPath();
      context.moveTo(left, y);
      context.lineTo(left + plotWidth, y);
      context.stroke();
      context.fillStyle = "#53647a";
      context.textAlign = "right";
      context.fillText(formatAxisTick(tick, yAxis.step), left - 10, y);
    }
    const xAxis = axisTicks(0, duration);
    for (const tick of xAxis.ticks) {
      const x = left + (plotWidth * tick) / duration;
      context.fillStyle = "#53647a";
      context.textAlign = "center";
      context.fillText(
        formatAxisTick(tick, xAxis.step),
        x,
        top + plotHeight + 20,
      );
    }
    context.fillText("time, s", left + plotWidth / 2, height - 10);
    context.save();
    context.beginPath();
    context.rect(left, top, plotWidth, plotHeight);
    context.clip();
    for (const index of chart.selected) {
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
    if (chart.selected.size === 0) {
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
    const timeStart = viewStart / 1e9;
    const timeDuration = viewDuration / 1e9;
    const timeAxis = axisTicks(timeStart, timeStart + timeDuration);
    for (const tick of timeAxis.ticks) {
      const fraction = (tick - timeStart) / timeDuration;
      const x = GUTTER + plotWidth * fraction;
      context.textAlign =
        fraction < 0.05 ? "left" : fraction > 0.95 ? "right" : "center";
      context.fillText(formatAxisTick(tick, timeAxis.step), x, RULER / 2);
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
  /** Shows only selected series from the hovered chart's units group. */
  function showChartTooltip(chart, event) {
    if (!profile) return;
    const bounds = chart.canvas.getBoundingClientRect();
    const left = Math.min(78, bounds.width / 4);
    const fraction =
      (event.clientX - bounds.left - left) /
      Math.max(1, bounds.width - left - 20);
    if (fraction < 0 || fraction > 1) return hideTooltip();
    const time = fraction * chartDuration();
    const lines = [`Time: ${formatNumber(time)} s`];
    for (const index of chart.selected) {
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
  }
  traceCanvas.addEventListener("mouseleave", hideTooltip);
  const chartObserver = new ResizeObserver(queueChart);
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
