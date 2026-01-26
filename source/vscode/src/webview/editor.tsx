// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/// <reference types="@types/vscode-webview"/>

const vscodeApi = acquireVsCodeApi();

import { render } from "preact";
import DOMPurify from "dompurify";
import {
  CircuitPanel,
  CircuitProps,
  detectThemeChange,
  updateStyleSheetTheme,
} from "qsharp-lang/ux";

import circuitHelloWorkerSource from "./circuitHelloWorker.inline";

window.addEventListener("message", onMessage);
window.addEventListener("load", main);

type CircuitState = {
  viewType: "circuit";
  props: CircuitProps;
};

type State = { viewType: "loading" } | CircuitState;
const loadingState: State = { viewType: "loading" };
let state: State = loadingState;

type CircuitHelloWorkerRequest =
  | { command: "hello"; reason?: string }
  | { command: "ping" };
type CircuitHelloWorkerResponse =
  | { command: "hello"; message: string; reason?: string }
  | { command: "pong" }
  | { command: "error"; error: { name: string; message: string } };

type ActiveHelloWorker = {
  requestId: number;
  worker: Worker;
  blobUrl?: string;
};

let activeHelloWorker: ActiveHelloWorker | null = null;
let helloRequestId = 0;

let lastSentCircuitJson: string | null = null;
let suppressNextCircuitHello = false;

function terminateActiveHelloWorker() {
  if (!activeHelloWorker) return;
  try {
    console.log("[qsharp] Terminating active circuit hello worker");
    activeHelloWorker.worker.terminate();
  } catch {
    // ignore
  }
  if (activeHelloWorker.blobUrl) {
    try {
      URL.revokeObjectURL(activeHelloWorker.blobUrl);
    } catch {
      // ignore
    }
  }
  activeHelloWorker = null;
}

function createHelloWorker(): { worker: Worker; blobUrl?: string } {
  const blobUrl = URL.createObjectURL(
    new Blob([circuitHelloWorkerSource], { type: "text/javascript" }),
  );
  return { worker: new Worker(blobUrl), blobUrl };
}

function requestHelloFromWorker(reason?: string) {
  // Cancel any in-flight worker work by terminating the previous worker.
  terminateActiveHelloWorker();

  const requestId = ++helloRequestId;
  const created = createHelloWorker();
  activeHelloWorker = {
    requestId,
    worker: created.worker,
    blobUrl: created.blobUrl,
  };

  created.worker.onmessage = (ev: MessageEvent<CircuitHelloWorkerResponse>) => {
    // Ignore late messages from cancelled workers.
    if (!activeHelloWorker || activeHelloWorker.requestId !== requestId) return;
    const msg = ev.data as any;
    if (!msg || typeof msg !== "object") return;
    switch (msg.command) {
      case "hello":
        console.log(
          `[qsharp] worker says: ${msg.message}${msg.reason ? ` (reason: ${msg.reason})` : ""}`,
        );
        terminateActiveHelloWorker();
        return;
      case "pong":
        // Keep the worker alive for now; currently unused.
        return;
      case "error":
        console.error("[qsharp] worker error", msg.error);
        terminateActiveHelloWorker();
        return;
    }
  };

  created.worker.postMessage({
    command: "hello",
    reason,
  } satisfies CircuitHelloWorkerRequest);
}

function main() {
  state = (vscodeApi.getState() as any) || loadingState;
  render(<App state={state} />, document.body);

  // The worker is created lazily; we only message it when the circuit changes.

  window.addEventListener("unload", () => {
    terminateActiveHelloWorker();
  });

  detectThemeChange(document.body, (isDark) =>
    updateStyleSheetTheme(
      isDark,
      "github-markdown-css",
      /(light\.css)|(dark\.css)/,
      "light.css",
      "dark.css",
    ),
  );
  readFromTextDocument();
}

function onMessage(event: any) {
  const message = event.data;
  if (!message?.command) {
    console.error("Unknown message: ", message);
    return;
  }
  switch (message.command) {
    case "error": {
      const sanitizedMessage = DOMPurify.sanitize(message.props.message);
      const sanitizedTitle = DOMPurify.sanitize(message.props.title);
      const innerHTML = `
        <div class="error">
          <h1>${sanitizedTitle}</h1>
          <p>${sanitizedMessage}</p>
        </div>
      `;
      document.body.innerHTML = innerHTML; // CodeQL [SM04949] message data is not untrusted, handler is running in an extension, and is sanitized.
      return;
    }
    case "circuit":
      {
        const prevCircuitJson =
          state.viewType === "circuit"
            ? JSON.stringify(state.props.circuit)
            : null;
        const nextCircuitJson = JSON.stringify(message.props.circuit);

        // Only short-circuit if both the circuit AND the dev toolbar flag are unchanged
        if (state.viewType === "circuit") {
          const sameCircuit =
            JSON.stringify(state.props.circuit) ===
            JSON.stringify(message.props.circuit);
          const prevToolbar = (state.props as any)?.showStateDevToolbar;
          const nextToolbar = (message.props as any)?.showStateDevToolbar;
          const sameToolbar = prevToolbar === nextToolbar;
          if (sameCircuit && sameToolbar) {
            return;
          }
        }

        // Trigger the worker only when the circuit changes.
        // Suppress the ping if this update is the echo of our own editCallback update.
        if (prevCircuitJson !== nextCircuitJson) {
          if (
            suppressNextCircuitHello &&
            lastSentCircuitJson === nextCircuitJson
          ) {
            suppressNextCircuitHello = false;
            lastSentCircuitJson = null;
          } else {
            requestHelloFromWorker("circuit changed");
          }
        }

        state = {
          viewType: "circuit",
          ...message,
        };
      }
      break;
    default:
      console.error("Unknown command: ", message.command);
      return;
  }

  vscodeApi.setState(state);
  render(<App state={state} />, document.body);
}

function readFromTextDocument() {
  vscodeApi.postMessage({ command: "read" });
}

function updateTextDocument(circuit: any) {
  // Only use the worker when the circuit is changing.
  // Mark this as a local change so we can avoid duplicating the hello
  // when VS Code echoes the updated circuit back to us.
  lastSentCircuitJson = JSON.stringify(circuit);
  suppressNextCircuitHello = true;
  requestHelloFromWorker("circuit changed");
  vscodeApi.postMessage({
    command: "update",
    text: JSON.stringify(circuit, null, 2),
  });
}

function runCircuit() {
  vscodeApi.postMessage({ command: "run" });
}

function App({ state }: { state: State }) {
  switch (state.viewType) {
    case "loading":
      return <div>Loading...</div>;
    case "circuit":
      return (
        <CircuitPanel
          {...state.props}
          isEditable={true}
          editCallback={updateTextDocument}
          runCallback={runCircuit}
        ></CircuitPanel>
      );
    default:
      console.error("Unknown view type in state", state);
      return <div>Loading error</div>;
  }
}
