// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { getCurrentCircuitModel } from "./events.js";
import type { ComponentGrid, Qubit } from "./circuit.js";
import {
  computeAmpMapForCircuit,
  type AmpMap,
  type Endianness,
} from "./stateComputeCore.js";

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
  return computeAmpMapForCircuit(model.qubits, model.componentGrid, endianness);
}
