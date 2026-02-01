// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { computeAmpMapForCircuit } from "../../../npm/qsharp/ux/circuit-vis/state-viz/stateComputeCore";
import { prepareStateVizColumnsFromAmpMap } from "../../../npm/qsharp/ux/circuit-vis/state-viz/stateVizPrep";

const LOG_PREFIX = "[qsharp][state-compute-worker]";
function log(...args: unknown[]) {
  // Intentionally using console.debug so this can be filtered easily.
  console.debug(LOG_PREFIX, ...args);
}

type Endianness = "big" | "little";

type CircuitModelSnapshot = {
  qubits: any[];
  componentGrid: any[];
};

type ComputeRequest = {
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

type ComputeResponse =
  | {
      command: "result";
      requestId: number;
      columns: any;
    }
  | {
      command: "error";
      requestId: number;
      error: { name: string; message: string };
    };

function respondError(requestId: number, err: unknown) {
  const error =
    err instanceof Error
      ? { name: err.name, message: err.message }
      : { name: "Error", message: String(err) };
  (self as any).postMessage({
    command: "error",
    requestId,
    error,
  } satisfies ComputeResponse);
}

(self as any).onmessage = (ev: MessageEvent<ComputeRequest>) => {
  const msg = ev.data as any;
  if (!msg || typeof msg !== "object") return;
  if (msg.command !== "compute") return;

  const requestId = typeof msg.requestId === "number" ? msg.requestId : 0;
  const startedAt = performance.now();

  try {
    const model = msg.model as CircuitModelSnapshot;
    const endianness = (msg.endianness as Endianness) ?? "big";

    const qubits = Array.isArray(model?.qubits) ? model.qubits.length : 0;
    const gridColumns = Array.isArray(model?.componentGrid)
      ? model.componentGrid.length
      : 0;
    log("compute started", {
      requestId,
      endianness,
      qubits,
      columns: gridColumns,
    });

    const ampMap = computeAmpMapForCircuit(
      model.qubits as any,
      model.componentGrid as any,
      endianness,
    );

    const elapsedMs = Math.round(performance.now() - startedAt);
    const opts = (msg.opts ?? {}) as any;
    const columns = prepareStateVizColumnsFromAmpMap(ampMap as any, opts);
    const colCount = Array.isArray(columns) ? columns.length : 0;
    const othersCount =
      Array.isArray(columns) &&
      columns.find((c: any) => c && c.isOthers === true)?.othersCount;
    log("compute finished", { requestId, elapsedMs, colCount, othersCount });
    (self as any).postMessage({
      command: "result",
      requestId,
      columns,
    } satisfies ComputeResponse);
  } catch (err) {
    const elapsedMs = Math.round(performance.now() - startedAt);
    log("compute failed", {
      requestId,
      elapsedMs,
      error:
        err instanceof Error
          ? { name: err.name, message: err.message }
          : { name: "Error", message: String(err) },
    });
    respondError(requestId, err);
  }
};
