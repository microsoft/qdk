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

const STATE_COMPUTE_LOG_PREFIX = "[qsharp][state-compute]";
function logStateCompute(...args: unknown[]) {
  // Intentionally using console.debug so this can be filtered easily.
  console.debug(STATE_COMPUTE_LOG_PREFIX, ...args);
}

type Endianness = "big" | "little";
type CircuitModelSnapshot = { qubits: any[]; componentGrid: any[] };
type WorkerComputeRequest = {
  command: "compute";
  requestId: number;
  model: CircuitModelSnapshot;
  endianness: Endianness;
  opts?: {
    normalize?: boolean;
    minProbThreshold?: number;
    maxColumns?: number;
  };
};
type WorkerComputeResponse =
  | { command: "result"; requestId: number; columns: any }
  | {
      command: "error";
      requestId: number;
      error: { name: string; message: string };
    };

type QsharpStateComputeApi = {
  computeStateVizColumnsForCircuitModel: (
    model: CircuitModelSnapshot,
    endianness: Endianness,
    opts?: {
      normalize?: boolean;
      minProbThreshold?: number;
      maxColumns?: number;
    },
  ) => Promise<any>;

  // Optional cleanup hook so new webview instances (or setting-driven redraws)
  // can dispose any previously-installed API and terminate its in-flight worker.
  dispose?: (reason?: string) => void;
};

type ActiveStateComputeWorker = {
  requestId: number;
  worker: Worker;
  blobUrl: string;
  startedAt: number;
  modelSummary?: { qubits: number; columns: number };
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

function cancelActiveStateComputeWorker(reason: string) {
  if (!activeStateComputeWorker) return;

  const { requestId, startedAt, modelSummary } = activeStateComputeWorker;
  const elapsedMs = Math.round(performance.now() - startedAt);
  logStateCompute("worker cancelled", {
    requestId,
    elapsedMs,
    reason,
    modelSummary,
  });

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

function installQsharpStateComputeApi() {
  const prev = (globalThis as any).qsharpStateComputeApi as
    | QsharpStateComputeApi
    | undefined;

  // If this webview script is re-initialized without a full page unload
  // (e.g., setting-driven recreation), ensure any previously-created worker is
  // cancelled/terminated so it can't deliver stale results later.
  try {
    prev?.dispose?.("replaced by new state compute API instance");
  } catch {
    // ignore
  }

  const api: QsharpStateComputeApi = {
    computeStateVizColumnsForCircuitModel: (
      model: CircuitModelSnapshot,
      endianness: Endianness,
      opts?: {
        normalize?: boolean;
        minProbThreshold?: number;
        maxColumns?: number;
      },
    ) => computeStateVizColumnsInWorker(model, endianness, opts),

    dispose: (reason?: string) => {
      cancelActiveStateComputeWorker(reason ?? "disposed");
    },
  };

  (globalThis as any).qsharpStateComputeApi = api;
}

function createStateComputeWorker(): { worker: Worker; blobUrl: string } {
  const blobUrl = URL.createObjectURL(
    new Blob([stateComputeWorkerSource], { type: "text/javascript" }),
  );
  return { worker: new Worker(blobUrl), blobUrl };
}

function computeStateVizColumnsInWorker(
  model: CircuitModelSnapshot,
  endianness: Endianness,
  opts?: {
    normalize?: boolean;
    minProbThreshold?: number;
    maxColumns?: number;
  },
) {
  cancelActiveStateComputeWorker("replaced by new compute request");

  const requestId = ++stateComputeRequestId;
  const startedAt = performance.now();
  const created = createStateComputeWorker();
  return new Promise<any>((resolve, reject) => {
    const modelSummary = {
      qubits: Array.isArray(model?.qubits) ? model.qubits.length : 0,
      columns: Array.isArray(model?.componentGrid)
        ? model.componentGrid.length
        : 0,
    };

    activeStateComputeWorker = {
      requestId,
      worker: created.worker,
      blobUrl: created.blobUrl,
      startedAt,
      modelSummary,
      reject,
    };

    logStateCompute("worker created", {
      requestId,
      blobUrl: created.blobUrl,
      endianness,
      modelSummary,
      mode: "viz",
    });

    created.worker.onerror = (ev: ErrorEvent) => {
      if (
        !activeStateComputeWorker ||
        activeStateComputeWorker.requestId !== requestId
      ) {
        return;
      }

      const elapsedMs = Math.round(performance.now() - startedAt);
      logStateCompute("worker onerror", {
        requestId,
        elapsedMs,
        message: ev.message,
        filename: (ev as any).filename,
        lineno: (ev as any).lineno,
        colno: (ev as any).colno,
      });

      disposeActiveStateComputeWorker();
      reject(new Error(ev.message || "State compute worker error"));
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
        const columns = msg.columns;
        const elapsedMs = Math.round(performance.now() - startedAt);
        const colCount = Array.isArray(columns) ? columns.length : 0;
        const others =
          Array.isArray(columns) &&
          columns.some((c: any) => c && c.isOthers === true);
        logStateCompute("compute finished", {
          requestId,
          elapsedMs,
          colCount,
          others,
        });
        disposeActiveStateComputeWorker();
        resolve(columns);
        return;
      }
      if (msg.command === "error") {
        const err = new Error(msg.error?.message ?? "Worker error");
        (err as any).name = msg.error?.name ?? "Error";
        const elapsedMs = Math.round(performance.now() - startedAt);
        logStateCompute("compute failed", {
          requestId,
          elapsedMs,
          error: { name: (err as any).name, message: err.message },
        });
        disposeActiveStateComputeWorker();
        reject(err);
      }
    };

    logStateCompute("compute started", {
      requestId,
      endianness,
      modelSummary: activeStateComputeWorker.modelSummary,
      opts,
    });

    created.worker.postMessage({
      command: "compute",
      requestId,
      model,
      endianness,
      opts,
    } satisfies WorkerComputeRequest);
  });
}

function main() {
  state = (vscodeApi.getState() as any) || loadingState;
  render(<App state={state} />, document.body);

  // Provide a host API so the circuit visualization can offload state computation
  // to a Web Worker without importing VS Code specific modules.
  installQsharpStateComputeApi();

  window.addEventListener("unload", () => {
    cancelActiveStateComputeWorker("webview unload");
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
