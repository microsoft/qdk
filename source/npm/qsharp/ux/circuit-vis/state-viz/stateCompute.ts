// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// State compute bridge for circuit-vis.
// Connects the current circuit model to the compute core, and optionally
// delegates async computation to a host-provided API (e.g. VS Code webview
// worker) with a main-thread fallback.

import { getCurrentCircuitModel } from "../events.js";
import type { ComponentGrid, Qubit } from "../circuit.js";
import { computeAmpMapForCircuit } from "./stateComputeCore.js";
import {
  prepareStateVizColumnsFromAmpMap,
  type PrepareStateVizOptions,
} from "./stateVizPrep.js";
import type { StateColumn } from "./stateViz.js";

type CircuitModelSnapshot = { qubits: Qubit[]; componentGrid: ComponentGrid };
type StateComputeHostApi = {
  computeStateVizColumnsForCircuitModel?: (
    model: CircuitModelSnapshot,
    opts: PrepareStateVizOptions,
  ) => Promise<StateColumn[]>;
};

function getHostStateComputeApi(): StateComputeHostApi | null {
  return (
    ((globalThis as any).qsharpStateComputeApi as StateComputeHostApi) ?? null
  );
}

export async function computeStateVizColumnsFromCurrentModelAsync(
  opts: PrepareStateVizOptions = {},
  expectedCircuitSvg?: SVGElement | null,
): Promise<StateColumn[] | null> {
  const model = getCurrentCircuitModel(expectedCircuitSvg);
  if (!model) return null;
  if (model.qubits.length === 0) return [];

  const api = getHostStateComputeApi();
  if (api?.computeStateVizColumnsForCircuitModel) {
    return await api.computeStateVizColumnsForCircuitModel(
      {
        qubits: model.qubits,
        componentGrid: model.componentGrid,
      },
      opts,
    );
  }

  // Fallback: compute and prepare on the main thread.
  const ampMap = computeAmpMapForCircuit(model.qubits, model.componentGrid);
  return prepareStateVizColumnsFromAmpMap(ampMap as any, opts);
}
