// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { getCurrentCircuitModel } from "./events.js";
import type { ComponentGrid, Qubit } from "./circuit.js";
import {
  computeAmpMapForCircuit,
  type AmpMap,
  type Endianness,
} from "./stateComputeCore.js";

const STATE_COMPUTE_LOG_PREFIX = "[qsharp][state-compute]";
let didLogMainThreadFallback = false;

function logMainThreadFallback(details: {
  endianness: Endianness;
  qubits: number;
  columns: number;
}) {
  if (didLogMainThreadFallback) return;
  didLogMainThreadFallback = true;

  // Keep unit tests quiet: in Node, `console.debug` is not typically filterable
  // and will show up in test output.
  const nodeProcess = (globalThis as any)?.process as any;
  const isNode = typeof nodeProcess !== "undefined" && !!nodeProcess?.versions?.node;
  if (isNode) return;

  if (typeof console === "undefined" || typeof console.debug !== "function") {
    return;
  }

  console.debug(
    STATE_COMPUTE_LOG_PREFIX,
    "falling back to main-thread state compute (no host worker API)",
    details,
  );
}

export { computeAmpMapForCircuit } from "./stateComputeCore.js";
export type { AmpMap, Endianness } from "./stateComputeCore.js";

export function computeAmpMapFromCurrentModel(
  endianness: Endianness = "big",
): AmpMap | null {
  const model = getCurrentCircuitModel();
  if (!model) return null;
  if (model.qubits.length === 0) return null;
  return computeAmpMapForCircuit(model.qubits, model.componentGrid, endianness);
}

type CircuitModelSnapshot = { qubits: Qubit[]; componentGrid: ComponentGrid };
type StateComputeHostApi = {
  computeAmpMapForCircuitModel?: (
    model: CircuitModelSnapshot,
    endianness: Endianness,
  ) => Promise<AmpMap>;
};

function getHostStateComputeApi(): StateComputeHostApi | null {
  return (
    ((globalThis as any).qsharpStateComputeApi as StateComputeHostApi) ?? null
  );
}

export async function computeAmpMapFromCurrentModelAsync(
  endianness: Endianness = "big",
): Promise<AmpMap | null> {
  const model = getCurrentCircuitModel();
  if (!model) return null;
  if (model.qubits.length === 0) return null;

  const api = getHostStateComputeApi();
  if (api?.computeAmpMapForCircuitModel) {
    return await api.computeAmpMapForCircuitModel(
      {
        qubits: model.qubits,
        componentGrid: model.componentGrid,
      },
      endianness,
    );
  }

  // Fallback: compute on the main thread if no host worker API is present.
  logMainThreadFallback({
    endianness,
    qubits: model.qubits.length,
    columns: model.componentGrid.length,
  });
  return computeAmpMapForCircuit(model.qubits, model.componentGrid, endianness);
}
