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
  renderLocations?: (s: SourceLocation[]) => { title: string; href: string };
  editor?: qviz.EditorHandlers;
}) {
  const isEditable = props.editor != null;
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
      (!isEditable && qubits === 0) ||
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
  renderLocations?: (s: SourceLocation[]) => { title: string; href: string };
  editor?: qviz.EditorHandlers;
}) {
  const circuitDiv = useRef<HTMLDivElement>(null);
  const qvizObj = useRef<ReturnType<typeof qviz.draw> | null>(null);
  const [zoomLevel, setZoomLevel] = useState(100);
  const [rendering, setRendering] = useState(true);

  const isEditable = props.editor != null;

  useEffect(() => {
    // Enable "rendering" text while the circuit is being drawn
    setRendering(true);
    const container = circuitDiv.current!;
    container.innerHTML = "";
  }, [props.circuitGroup]);

  useEffect(() => {
    if (rendering) {
      const container = circuitDiv.current!;
      // Draw the circuits - may take a while for large circuits
      qvizObj.current = qviz.draw(props.circuitGroup, container, {
        renderLocations: props.renderLocations,
        editor: props.editor,
        onZoomChange: (zoom) => {
          setZoomLevel(zoom);
        },
      });

      // Disable "rendering" text
      setRendering(false);
    }
  }, [rendering]);

  return (
    <div>
      <div>
        {isEditable || rendering ? null : (
          <ZoomControl zoom={zoomLevel} onInput={userSetZoomLevel} />
        )}
      </div>
      <div>
        {rendering
          ? `Rendering diagram with ${props.circuitGroup.circuits[0].componentGrid.length} gates...`
          : ""}
      </div>
      <div class="qs-circuit" ref={circuitDiv}></div>
    </div>
  );

  function userSetZoomLevel(zoomLevel: number) {
    if (qvizObj.current && circuitDiv.current) {
      qvizObj.current.userSetZoomLevel(zoomLevel);
    }
  }
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
  const isEditable = props.editor != null;
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
        {isEditable ? (
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
          renderLocations={renderLocations}
          editor={props.editor}
        ></Circuit>
      ) : null}
    </div>
  );
}

function renderLocations(locations: SourceLocation[]) {
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
      location.file.replace(/\/+$/, "").split("/").pop() ?? location.file;
    const title = `${basename}:${location.line + 1}:${location.column + 1}`;
    return title;
  });
  const title = titles.length > 1 ? `${titles[0]}, ...` : titles[0];

  const argsStr = encodeURI(encodeURIComponent(JSON.stringify([qdkLocations])));
  const href = `command:qsharp-vscode.gotoLocations?${argsStr}`;
  return {
    title,
    href,
  };
}
