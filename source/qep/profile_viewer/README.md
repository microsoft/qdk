# QEP profile viewer

Open `index.html` directly in a modern browser. Keep `viewer.js` next to it.
There is no build step, server, package installation, network request, or CDN
dependency. The page uses native Canvas 2D rather than a charting framework so it
also works offline over `file://`.

## Loading profiles

- **Open JSON file** loads a serialized `ProfilerResult`.
- Files are read locally with the browser File API and are never uploaded.
- Invalid JSON or malformed profile fields produce a visible error. The last
  successfully loaded profile remains displayed if another file fails to load.

## Summary and rate chart

The summary shows logical qubits, reported physical qubits, runtime in seconds,
and the exported operation/event count. Expand **Per-operation totals** for the
two per-operation maps. Informational messages are displayed as plain text.
Zero physical qubits and empty maps are displayed as reported, not estimated.
The temporal profiler's operation count can include factory/resource events,
not just quantum instructions.

The rate chart has time in seconds on its x axis and a shared numeric y axis
without a unit label. The right-hand checklist supplies each series' units and
allows individual selection, selecting all, or clearing all. Hover to see values.
The exported aggregation window is displayed above the chart.

Rates are not recomputed: in particular the final partial window retains the
exporter's full-window denominator. Series with different units share the same
scale; hide unrelated series when comparing them. Large series use a
pixel-column envelope that retains extrema and bounds the number of points drawn.

## Execution trace

**Show trace** is initially off. A null trace explains how to enable
`ProfilerConfig.record_trace`; an empty trace is identified separately.

When shown, logical qubits appear first in numeric order, followed by factories.
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

An interval index is built lazily on first show, yielding between chunks.
Per-lane start-time arrays and prefix maximum end times allow binary-search
range queries, including long intervals that begin before the viewport.
Empty logical-qubit lanes are synthesized without creating a per-qubit index.

Hiding the trace cancels pending indexing/redraw frames, disconnects its resize
observer, clears hover geometry, and releases the canvas backing store.
No trace animation loop or index construction runs while hidden. A completed
index is retained for quick reopening, and released on loading another profile.
The parsed JSON remains in memory; hiding a trace does not undo the initial
file read/JSON parse. Extremely large exports can still require substantial
memory for the parsed data and lane index.

## Implementation

`index.html` contains the layout and styles. `viewer.js` contains validation,
file loading, the chart, and the trace index/renderer. It is a classic script,
not an ES module, which avoids local-file module/CORS restrictions.
The page and canvases use a white background with dark text and light grid lines;
instruction colors remain consistent between profiles.
The data helpers also expose CommonJS exports for focused Node-based tests,
without affecting standalone browser use.
