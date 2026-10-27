# QEP profile viewer

Open `index.html` directly in a modern browser. Keep `viewer.js` next to it.
There is no build step, required server, package installation, or CDN
dependency. The page uses native Canvas 2D rather than a charting framework so it
also works offline over `file://` with the file picker.

## Loading profiles

- **Open JSON file** loads a serialized `ProfilerResult`.
- Files are read locally with the browser File API and are never uploaded.
- Invalid JSON or malformed profile fields produce a visible error. The last
  successfully loaded profile remains displayed if another file fails to load.
- Exports must include `totals.logical_compute_qubits`; regenerate older profiles
  to distinguish compute qubits from factory qubits.
- When served over HTTP or HTTPS, `index.html?profile_name=my_run` automatically
  loads `profiles/my_run.json` from the directory next to `index.html`.
  Names may contain only ASCII letters, digits, underscores, and hyphens; omit
  the `.json` extension. Loading failures are shown in the status message.
  Automatic loading is unavailable over `file://`; use **Open JSON file** instead.

## Summary and rate chart

The summary shows total logical qubits, logical compute qubits, reported physical qubits, runtime in seconds,
and the exported operation/event count. Expand **Per-operation totals** for the
two per-operation maps. Informational messages are displayed as plain text.
Zero physical qubits and empty maps are displayed as reported, not estimated.
The temporal profiler's operation count can include factory/resource events,
not just quantum instructions.

Time series are grouped by their exact units string, with a separate chart and
independent y scale for each group. Each chart has time in seconds on its x axis.
The right-hand checklist contains only that group's series and allows individual
selection, selecting all, or clearing all. Hover to see values from that group.
Each chart has its own panel titled with its units, with the exported aggregation
window displayed beside the title.
Chart axes and the trace time axis use 3-10 ticks at rounded steps such as 1, 2,
or 5 times a power of ten, preferring fewer decimal places. Horizontal grid lines
follow the chart's y-axis ticks.

Rates are not recomputed: in particular the final partial window retains the
exporter's full-window denominator. For example, `events/s` and `factories` are
shown on different charts. Large series use a
pixel-column envelope that retains extrema and bounds the number of points drawn.

## Execution trace

The execution trace section is hidden when the export's trace is null.
Recorded traces are displayed automatically, without a checkbox. An empty
recorded trace shows an explanatory message instead of a timeline.

When shown, `totals.logical_compute_qubits` compute-qubit lanes appear first in
numeric order, followed by factories. `totals.logical_qubits` includes factory
qubits and is not used to size the compute-qubit portion of the trace.
Each instruction is drawn on every qubit/factory operand it touches, with no
connecting lines. For example, CNOT appears on two qubit lanes and an injection
appears on its qubit and factory lanes. Result IDs and floating-point operands
do not create lanes. Instructions with no qubit/factory operands, such as
`read_result`, are counted in an explanatory note rather than assigned a
misleading lane.

Rectangles cover the instruction's start/end interval. Zero-duration operations
receive a one-pixel marker. Known instruction names use a hardcoded color
palette; other names receive deterministic generated colors. Hover for the full
instruction, operands, timestamps, and duration. Labels disappear when they do
not fit. The timeline includes pending factory rounds, so it may extend beyond
the algorithm runtime shown in the summary.

- Scroll horizontally and vertically to pan through the trace.
- **Zoom in** and **Zoom out** preserve the current viewport's center.
- Ctrl/Cmd + mouse wheel zooms around the pointer.
- **Fit trace** restores the entire recorded time extent.

The canvas backing store is only viewport-sized. The renderer draws visible
lanes and time-overlapping intervals, not a DOM element per instruction.
The native horizontal scrollbar is capped in size and mapped to the logical
time range, avoiding browser limits on enormous CSS widths.

An interval index is built automatically when a recorded trace is loaded, yielding between chunks.
Per-lane start-time arrays and prefix maximum end times allow binary-search
range queries, including long intervals that begin before the viewport.
Empty logical-qubit lanes are synthesized without creating a per-qubit index.

Loading another profile cancels pending indexing/redraw frames, disconnects the
trace resize observer, clears hover geometry, and releases the previous canvas
backing store and index. No trace animation loop or index construction runs
when the export has no trace. Extremely large exports can still require
substantial memory for the parsed data and lane index.

## Implementation

`index.html` contains the layout and styles. `viewer.js` contains validation,
file loading, the chart, and the trace index/renderer. It is a classic script,
not an ES module, which avoids local-file module/CORS restrictions.
The page and canvases use a white background with dark text and light grid lines;
instruction colors remain consistent between profiles.
The data helpers also expose CommonJS exports for focused Node-based tests,
without affecting standalone browser use.
