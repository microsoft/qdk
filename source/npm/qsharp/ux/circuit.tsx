// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as qviz from "./circuit-vis/index.js";
import { useEffect, useRef, useState } from "preact/hooks";
import { CircuitProps } from "./data.js";
import { Spinner } from "./spinner.js";
import { SourceLocation, toCircuitGroup } from "./circuit-vis/circuit.js";

// For perf reasons we set a limit on how many gates/qubits
// we attempt to render. This is still a lot higher than a human would
// reasonably want to look at, but it takes about a second to
// render a circuit this big on a mid-grade laptop so we allow it.
const MAX_OPERATIONS = 10000;
const MAX_QUBITS = 1000;

// For now we only support one circuit at a time.
const MAX_CIRCUITS = 1;

// This component is shared by the Python widget and the VS Code panel
export function Circuit(props: {
  circuit?: qviz.CircuitGroup | qviz.Circuit;
  isEditable: boolean;
  editCallback?: (fileData: qviz.CircuitGroup) => void;
  runCallback?: () => void;
  renderLocations: (s: SourceLocation[]) => { title: string; href: string };
}) {
  let unrenderable = false;
  let qubits = 0;
  let operations = 0;
  let errorMsg: string | undefined = undefined;

  const result = toCircuitGroup(props.circuit);
  if (result.ok) {
    const circuit = result.circuitGroup.circuits[0];
    if (circuit.componentGrid === undefined) circuit.componentGrid = [];
    if (circuit.qubits === undefined) circuit.qubits = [];
    qubits = circuit.qubits.length;
    operations = circuit.componentGrid.length;

    unrenderable =
      unrenderable ||
      result.circuitGroup.circuits.length > MAX_CIRCUITS ||
      (!props.isEditable && qubits === 0) ||
      operations > MAX_OPERATIONS ||
      qubits > MAX_QUBITS;
  } else {
    errorMsg = result.error;
  }

  return (
    <div>
      {!result.ok || unrenderable ? (
        <Unrenderable
          qubits={qubits}
          operations={operations}
          error={errorMsg}
        />
      ) : (
        <ZoomableCircuit {...props} circuitGroup={result.circuitGroup} />
      )}
    </div>
  );
}

function ZoomableCircuit(props: {
  circuitGroup: qviz.CircuitGroup;
  isEditable: boolean;
  editCallback?: (fileData: qviz.CircuitGroup) => void;
  runCallback?: () => void;
  renderLocations: (s: SourceLocation[]) => { title: string; href: string };
}) {
  const circuitDiv = useRef<HTMLDivElement>(null);
  const [zoomLevel, setZoomLevel] = useState(100);
  const [rendering, setRendering] = useState(true);
  const [zoomOnResize, setZoomOnResize] = useState(true);
  const [expandDepth, setExpandDepth] = useState(0);

  const { circuitDepth, numGates } = getSize(
    props.circuitGroup.circuits.map((c) => c.componentGrid),
  );

  useEffect(() => {
    // Enable "rendering" text while the circuit is being drawn
    setRendering(true);
    const container = circuitDiv.current!;
    container.innerHTML = "";
  }, [props.circuitGroup, expandDepth]);

  useEffect(() => {
    if (rendering) {
      const container = circuitDiv.current!;
      // Draw the circuits - may take a while for large circuits
      const svg = renderCircuits(
        props.circuitGroup,
        container,
        expandDepth,
        props.isEditable,
        props.renderLocations,
        props.editCallback,
        props.runCallback,
      );

      if (!props.isEditable) {
        const initialZoom = calculateZoomToFit(container, svg as SVGElement);
        // Set the initial zoom level
        setZoomLevel(initialZoom);
        // Resize the SVG to fit
        updateWidth();
      }

      // Calculate the initial zoom level based on the container width
      // Disable "rendering" text
      setRendering(false);
    } else if (!props.isEditable) {
      // Initial drawing done, attach window resize handler
      window.addEventListener("resize", onResize);
      return () => {
        window.removeEventListener("resize", onResize);
      };
    }
  }, [rendering, zoomOnResize]);

  useEffect(() => {
    updateWidth();
  }, [zoomLevel]);
  return (
    <div>
      <div>
        {props.isEditable || rendering ? null : (
          <DepthControl
            max={circuitDepth}
            initial={expandDepth}
            onInput={setExpandDepth}
          />
        )}
        {props.isEditable || rendering ? null : (
          <ZoomControl zoom={zoomLevel} onInput={userSetZoomLevel} />
        )}
      </div>
      <div
        className={rendering ? "rendering-message" : ""}
        style={{
          opacity: 0,
          transition: "opacity 0.1s ease-in",
          animationDelay: "1s",
          animationDuration: "0.1s",
          animationFillMode: "forwards",
        }}
      >
        <style>
          {`
            .rendering-message {
              animation: showMessage 0.1s ease-in 1s forwards;
            }
            @keyframes showMessage {
              to { opacity: 1; }
            }
          `}
        </style>
        {rendering ? `Rendering diagram with ${numGates} gates...` : ""}
      </div>
      <div class="qs-circuit" ref={circuitDiv}></div>
    </div>
  );

  /**
   * Window resize handler to recalculate and set the zoom level
   * based on the new window width.
   */
  function onResize() {
    if (!zoomOnResize) {
      return;
    }

    const [container, svg] = [circuitDiv.current, currentSvg()];
    if (container && svg) {
      // Recalculate the zoom level based on the container width
      const initialZoom = calculateZoomToFit(container, svg);
      // Set the zoom level
      setZoomLevel(initialZoom);
    }
  }

  /**
   * Update the width of the SVG element based on the current zoom level.
   */
  function updateWidth() {
    const svg = currentSvg();
    if (svg) {
      // The width attribute contains the true width, generated by qviz.
      // We'll leave this attribute untouched, so we can use it again if the
      // zoom level is ever updated.
      const width = svg.getAttribute("width")!;

      // We'll set the width in the style attribute to (true width * zoom level).
      // This value takes precedence over the true width in the width attribute.
      svg.setAttribute(
        "style",
        `max-width: ${width}; width: ${(parseInt(width) * (zoomLevel || 100)) / 100}; height: auto`,
      );
    }
  }

  function renderCircuits(
    circuitGroup: qviz.CircuitGroup,
    container: HTMLDivElement,
    expandDepth: number,
    isEditable: boolean,
    renderLocations?: (s: SourceLocation[]) => { title: string; href: string },
    editCallback?: (fileData: qviz.CircuitGroup) => void,
    runCallback?: () => void,
  ) {
    qviz.draw(circuitGroup, container, {
      renderDepth: expandDepth,
      isEditable,
      editCallback,
      runCallback,
      renderLocations,
    });
    return container.getElementsByClassName("qviz")[0]!;
  }

  /**
   * Calculate the zoom level that will fit the circuit into the current size of the container.
   */
  function calculateZoomToFit(container: HTMLDivElement, svg: SVGElement) {
    const containerWidth = container.clientWidth;
    // width and height are the true dimensions generated by qviz
    const width = parseInt(svg.getAttribute("width")!);
    const height = svg.getAttribute("height")!;

    svg.setAttribute("viewBox", `0 0 ${width} ${height}`);
    const zoom = Math.min(Math.ceil((containerWidth / width) * 100), 100);
    // never auto-zoom lower than 70%
    return Math.max(zoom, 70);
    // return zoom;/
  }

  function currentSvg(): SVGElement | undefined {
    return circuitDiv.current?.querySelector(".qviz") ?? undefined;
  }

  function userSetZoomLevel(zoom: number) {
    setZoomOnResize(false);
    setZoomLevel(zoom);
  }
}

function DepthControl(props: {
  max: number;
  initial: number;
  onInput: (depth: number) => void;
}) {
  const [currentDepth, setCurrentDepth] = useState(props.initial);

  useEffect(() => {
    props.onInput(currentDepth);
  }, [currentDepth]);

  return (
    <p>
      {/* <label htmlFor="qs-circuit-render-depth">Expand</label> */}
      <button
        onClick={() => {
          const newDepth = Math.max(0, currentDepth - 1);
          setCurrentDepth(newDepth);
        }}
        disabled={currentDepth === 0}
      >
        Collapse -
      </button>
      <button
        onClick={() => {
          const newDepth = Math.min(props.max, currentDepth + 1);
          setCurrentDepth(newDepth);
        }}
        disabled={currentDepth === props.max}
      >
        Expand +
      </button>
      {/* <input
        id="qs-circuit-render-depth"
        type="number"
        min="0"
        max={props.max}
        step="1"
        value={currentDepth}
        onInput={(e) =>
          props.onInput(parseInt((e.target as HTMLInputElement).value) || 0)
        }
      /> */}
    </p>
  );
}

function Unrenderable(props: {
  qubits: number;
  operations: number;
  error?: string;
}) {
  let errorDiv = null;

  if (props.error) {
    errorDiv = (
      <div>
        <p>
          <b>Unable to render circuit:</b>
        </p>
        <pre>{props.error}</pre>
      </div>
    );
  } else if (props.qubits === 0) {
    errorDiv = (
      <div>
        <p>No circuit to display. No qubits have been allocated.</p>
      </div>
    );
  } else if (props.operations > MAX_OPERATIONS) {
    // Don't show the real number of operations here, as that number is
    // *already* truncated by the underlying circuit builder.
    errorDiv = (
      <div>
        <p>
          This circuit has too many gates to display. The maximum supported
          number of gates is {MAX_OPERATIONS}.
        </p>
      </div>
    );
  } else if (props.qubits > MAX_QUBITS) {
    errorDiv = (
      <div>
        <p>
          This circuit has too many qubits to display. It has {props.qubits}{" "}
          qubits, but the maximum supported is {MAX_QUBITS}.
        </p>
      </div>
    );
  }

  return <div class="qs-circuit-error">{errorDiv}</div>;
}

function ZoomControl(props: { zoom: number; onInput: (zoom: number) => void }) {
  return (
    <p>
      <label htmlFor="qs-circuit-zoom">Zoom </label>
      <input
        id="qs-circuit-zoom"
        type="number"
        min="10"
        max="100"
        step="10"
        value={props.zoom}
        onInput={(e) =>
          props.onInput(parseInt((e.target as HTMLInputElement).value) || 0)
        }
      />
      %
    </p>
  );
}

// This component is exclusive to the VS Code panel
export function CircuitPanel(props: CircuitProps) {
  const error = props.errorHtml ? (
    <div>
      <p>
        {props.circuit
          ? "The program encountered a failure. See the error(s) below."
          : "A circuit could not be generated for this program. See the error(s) below."}
        <br />
      </p>
      <div dangerouslySetInnerHTML={{ __html: props.errorHtml }}></div>
    </div>
  ) : null;

  return (
    <div class="qs-circuit-panel">
      <div>
        <h1>
          {props.title} {props.simulated ? "(Trace)" : ""}
        </h1>
      </div>
      {error && <div class="qs-circuit-error">{error}</div>}
      {props.targetProfile && <p>{props.targetProfile}</p>}
      {props.simulated && (
        <p>
          WARNING: This diagram shows the result of tracing a dynamic circuit,
          and may change from run to run.
        </p>
      )}
      <p>
        Learn more at{" "}
        {props.isEditable ? (
          <a href="https://aka.ms/qdk.circuit-editor">
            https://aka.ms/qdk.circuit-editor
          </a>
        ) : (
          <a href="https://aka.ms/qdk.circuits">https://aka.ms/qdk.circuits</a>
        )}
      </p>
      {props.calculating ? (
        <div>
          <Spinner />
        </div>
      ) : null}
      {props.circuit ? (
        <Circuit
          circuit={props.circuit}
          isEditable={props.isEditable}
          editCallback={props.editCallback}
          runCallback={props.runCallback}
          renderLocations={(locations) => {
            const qdkLocations = locations.map((location) => {
              const position = {
                line: location.line,
                character: location.column,
              };
              return {
                source: location.file,
                span: {
                  start: position,
                  end: position,
                },
              };
            });

            const titles = locations.map((location) => {
              const basename =
                location.file.replace(/\/+$/, "").split("/").pop() ??
                location.file;
              const title = `${basename}:${location.line}:${location.column}`;
              return title;
            });
            const title = titles.length > 1 ? `${titles[0]}, ...` : titles[0];

            const argsStr = encodeURIComponent(JSON.stringify([qdkLocations]));
            const href = `command:qsharp-vscode.gotoLocations?${argsStr}`;
            return {
              title,
              href,
            };
          }}
        ></Circuit>
      ) : null}
    </div>
  );
}
function getSize(grids: qviz.ComponentGrid[]): {
  circuitDepth: number;
  numGates: number;
} {
  // Return the maximum componentGrid depth (number of "columns"/steps)
  // across all circuits in the group. Be defensive about missing fields.
  if (!grids || !Array.isArray(grids)) return { circuitDepth: 0, numGates: 0 };

  let numGates = 0;
  let maxDepth = 0;
  for (const componentGrid of grids) {
    for (const column of componentGrid ?? []) {
      for (const component of column.components ?? []) {
        numGates += 1;
        if (component.children && component.children.length > 0) {
          const { circuitDepth: childDepth, numGates: childNumGates } = getSize(
            [component.children],
          );
          numGates += childNumGates;
          if (childDepth + 1 > maxDepth) {
            maxDepth = childDepth + 1;
          }
        }
      }
    }
  }

  return {
    circuitDepth: maxDepth,
    numGates,
  };
}
