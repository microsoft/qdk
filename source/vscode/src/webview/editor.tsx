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

import stateComputeWorkerSource from "./stateComputeWorker.inline.ts";

window.addEventListener("message", onMessage);
window.addEventListener("load", main);

type CircuitState = {
  viewType: "circuit";
  props: CircuitProps;
};

type State = { viewType: "loading" } | CircuitState;
const loadingState: State = { viewType: "loading" };
let state: State = loadingState;

type Endianness = "big" | "little";
type CircuitModelSnapshot = { qubits: any[]; componentGrid: any[] };
type WorkerComputeRequest = {
  command: "compute";
  requestId: number;
  model: CircuitModelSnapshot;
  endianness: Endianness;
};
type WorkerComputeResponse =
  | { command: "result"; requestId: number; ampMap: any }
  | {
      command: "error";
      requestId: number;
      error: { name: string; message: string };
    };

type ActiveStateComputeWorker = {
  requestId: number;
  worker: Worker;
  blobUrl: string;
  reject?: (err: unknown) => void;
};

let activeStateComputeWorker: ActiveStateComputeWorker | null = null;
let stateComputeRequestId = 0;

function disposeActiveStateComputeWorker() {
  if (!activeStateComputeWorker) return;
  try {
    activeStateComputeWorker.worker.terminate();
  } catch {
    // ignore
  }
  try {
    URL.revokeObjectURL(activeStateComputeWorker.blobUrl);
  } catch {
    // ignore
  }
  activeStateComputeWorker = null;
}

function cancelActiveStateComputeWorker() {
  if (!activeStateComputeWorker) return;
  const reject = activeStateComputeWorker.reject;
  // Prevent any later cleanup from changing the settled promise.
  activeStateComputeWorker.reject = undefined;
  disposeActiveStateComputeWorker();
  try {
    reject?.(new DOMException("State compute cancelled", "AbortError"));
  } catch {
    // ignore
  }
}

function createStateComputeWorker(): { worker: Worker; blobUrl: string } {
  const blobUrl = URL.createObjectURL(
    new Blob([stateComputeWorkerSource], { type: "text/javascript" }),
  );
  return { worker: new Worker(blobUrl), blobUrl };
}

function computeAmpMapInWorker(
  model: CircuitModelSnapshot,
  endianness: Endianness,
) {
  cancelActiveStateComputeWorker();

  const requestId = ++stateComputeRequestId;
  const created = createStateComputeWorker();
  return new Promise<any>((resolve, reject) => {
    activeStateComputeWorker = {
      requestId,
      worker: created.worker,
      blobUrl: created.blobUrl,
      reject,
    };

    created.worker.onmessage = (ev: MessageEvent<WorkerComputeResponse>) => {
      if (
        !activeStateComputeWorker ||
        activeStateComputeWorker.requestId !== requestId
      ) {
        return;
      }
      const msg = ev.data as any;
      if (!msg || typeof msg !== "object") return;
      if (msg.command === "result") {
        const ampMap = msg.ampMap;
        disposeActiveStateComputeWorker();
        resolve(ampMap);
        return;
      }
      if (msg.command === "error") {
        const err = new Error(msg.error?.message ?? "Worker error");
        (err as any).name = msg.error?.name ?? "Error";
        disposeActiveStateComputeWorker();
        reject(err);
      }
    };

    created.worker.postMessage({
      command: "compute",
      requestId,
      model,
      endianness,
    } satisfies WorkerComputeRequest);
  });
}

function main() {
  state = (vscodeApi.getState() as any) || loadingState;
  render(<App state={state} />, document.body);

  // Provide a host API so the circuit visualization can offload state computation
  // to a Web Worker without importing VS Code specific modules.
  (globalThis as any).qsharpStateComputeApi = {
    computeAmpMapForCircuitModel: (
      model: CircuitModelSnapshot,
      endianness: Endianness,
    ) => computeAmpMapInWorker(model, endianness),
  };

  window.addEventListener("unload", () => {
    cancelActiveStateComputeWorker();
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
