// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { computeAmpMapForCircuit } from "../../../npm/qsharp/ux/circuit-vis/stateComputeCore";

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
};

type ComputeResponse =
  | {
      command: "result";
      requestId: number;
      ampMap: any;
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
    const endianness = (msg.endianness as Endianness) ?? "big";

    const ampMap = computeAmpMapForCircuit(
      model.qubits as any,
      model.componentGrid as any,
      endianness,
    );

    (self as any).postMessage({
      command: "result",
      requestId,
      ampMap,
    } satisfies ComputeResponse);
  } catch (err) {
    respondError(requestId, err);
  }
};
