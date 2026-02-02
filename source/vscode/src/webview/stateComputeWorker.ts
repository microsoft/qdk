// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { computeAmpMapForCircuit } from "../../../npm/qsharp/ux/circuit-vis/state-viz/stateComputeCore";
import { prepareStateVizColumnsFromAmpMap } from "../../../npm/qsharp/ux/circuit-vis/state-viz/stateVizPrep";

type CircuitModelSnapshot = {
  qubits: any[];
  componentGrid: any[];
};

type ComputeRequest = {
  command: "compute";
  requestId: number;
  model: CircuitModelSnapshot;
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

  try {
    const model = msg.model as CircuitModelSnapshot;

    const ampMap = computeAmpMapForCircuit(model.qubits as any, model.componentGrid as any);
    const opts = (msg.opts ?? {}) as any;
    const columns = prepareStateVizColumnsFromAmpMap(ampMap as any, opts);
    (self as any).postMessage({
      command: "result",
      requestId,
      columns,
    } satisfies ComputeResponse);
  } catch (err) {
    respondError(requestId, err);
  }
};
