# Profile viewer

Open `index.html` directly in a modern browser. Keep `viewer.js` next to it.
There is no build step and nothing to install. The page uses native Canvas 2D
with no charting framework, server or CDN dependency. It works offline over
`file://` with the file picker.

## Loading profiles

- Choose **Open JSON file** to load a serialized `ProfilerResult`.
- Files are read locally with the browser File API and are never uploaded.
- If JSON or profile fields are malformed, the viewer shows an error and keeps
  the last successfully loaded profile on screen.
- Exports must include `totals.logical_compute_qubits`; regenerate older profiles
  to distinguish compute qubits from factory qubits.
- Each time series must include a string `group_name`, which determines its chart
  and chart title.
- When served over HTTP or HTTPS, `index.html?profile_name=my_run` automatically
  loads `profiles/my_run.json` from the directory next to `index.html`.
  Names may contain only ASCII letters, digits, underscores, and hyphens; omit
  the `.json` extension. Loading failures are shown in the status message.
  Automatic loading is unavailable over `file://`; use **Open JSON file** instead.

## Summary and rate chart

The summary shows total logical qubits, logical compute qubits, reported physical
qubits, runtime in seconds and the exported operation/event count. Expand
**Per-operation totals** to see the two per-operation maps. Informational messages
appear as plain text. The viewer preserves reported values, including zero
physical qubits and empty maps; it does not fill in estimates. The temporal
profiler's operation count can include factory/resource events as well as
quantum instructions.

Time series are grouped by their exact `group_name`, with a separate chart and
independent y scale for each group. Each chart has time in seconds on its x axis.
The checklist on the right contains only that group's series. You can select
individual series, select all or clear all. Hover to see values from the group.
The panel title is the group name, with the exported aggregation window beside it.
Chart axes and the trace time axis use 3-10 ticks at rounded steps such as 1, 2,
or 5 times a power of ten, preferring fewer decimal places. Horizontal grid lines
follow the chart's y-axis ticks.

The viewer uses the exported rates without recomputing them, including the
full-window denominator for the final partial window. For example, `Event rates`
and `Utilization` appear on different charts. Each series shows its units.
For large series, a pixel-column envelope preserves extrema while limiting the
number of points drawn.

## Execution trace

The viewer hides the execution trace section when the export's trace is null.
Recorded traces appear automatically, without a checkbox. If a recorded trace
is empty, the section shows an explanatory message in place of the timeline.

When shown, `totals.logical_compute_qubits` compute-qubit lanes appear first in
numeric order, followed by factories. `totals.logical_qubits` includes factory
qubits and is not used to size the compute-qubit portion of the trace.
Each instruction is drawn on every qubit/factory operand it touches, with no
connecting lines. For example, CNOT appears on two qubit lanes and an injection
appears on its qubit and factory lanes. Result IDs and floating-point operands
do not create lanes. Instructions with no qubit/factory operands, such as
`read_result`, have no lane. An explanatory note gives their count.

Rectangles cover each instruction's start/end interval. A one-pixel marker shows
zero-duration operations. Known instruction names use a hardcoded color palette;
other names get deterministic generated colors. Hover for the full
instruction, operands, timestamps, and duration. Labels disappear when they do
not fit. The timeline includes pending factory rounds, so it may extend beyond
the algorithm runtime shown in the summary.

- Scroll horizontally and vertically to pan through the trace.
- **Zoom in** and **Zoom out** preserve the current viewport's center.
- Ctrl/Cmd + mouse wheel zooms around the pointer.
- **Fit trace** restores the entire recorded time extent.

The canvas backing store is the size of the viewport. The renderer draws visible
lanes and time-overlapping intervals without creating a DOM element per
instruction. The native horizontal scrollbar has a capped size mapped to the
logical time range, so browser limits on CSS widths do not limit the trace.

When a recorded trace loads, the viewer builds an interval index in chunks,
yielding between them.
Per-lane start-time arrays and prefix maximum end times allow binary-search
range queries, including long intervals that begin before the viewport.
The viewer creates empty logical-qubit lanes without a per-qubit index.

Loading another profile cancels pending indexing/redraw frames, disconnects the
trace resize observer, clears hover geometry, and releases the previous canvas
backing store and index. No trace animation loop or index construction runs
when the export has no trace. Extremely large exports can still require
substantial memory for the parsed data and lane index.

## Implementation

`index.html` contains the layout and styles. `viewer.js` handles validation,
file loading, the chart and the trace index/renderer. It uses a classic script
to avoid the local-file module/CORS restrictions of ES modules.
The page and canvases use a white background with dark text and light grid lines;
instruction colors remain consistent between profiles.
The data helpers also expose CommonJS exports for focused Node-based tests,
without affecting standalone browser use.

## TODO

Reimplement this viewer in TypeScript as a widget and/or using Python libraries like
matplotlib/plotly for use in notebooks.
