// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// State compute bridge for circuit-vis.
// Connects the current circuit model to the compute core, and optionally
// delegates async computation to a host-provided API (e.g. VS Code webview
// worker) with a main-thread fallback.

import { getCurrentCircuitModel } from "../events.js";
import type { ComponentGrid, Qubit } from "../circuit.js";
import {
  computeAmpMapForCircuit,
  type Endianness,
} from "./stateComputeCore.js";
import {
  prepareStateVizColumnsFromAmpMap,
  type PrepareStateVizOptions,
} from "./stateVizPrep.js";
import type { StateColumn } from "./stateViz.js";

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
  const isNode =
    typeof nodeProcess !== "undefined" && !!nodeProcess?.versions?.node;
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

type CircuitModelSnapshot = { qubits: Qubit[]; componentGrid: ComponentGrid };
type StateComputeHostApi = {
  computeStateVizColumnsForCircuitModel?: (
    model: CircuitModelSnapshot,
    endianness: Endianness,
    opts: PrepareStateVizOptions,
  ) => Promise<StateColumn[]>;
};

function getHostStateComputeApi(): StateComputeHostApi | null {
  return (
    ((globalThis as any).qsharpStateComputeApi as StateComputeHostApi) ?? null
  );
}

export async function computeStateVizColumnsFromCurrentModelAsync(
  endianness: Endianness = "big",
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
      endianness,
      opts,
    );
  }

  // Fallback: compute and prepare on the main thread.
  logMainThreadFallback({
    endianness,
    qubits: model.qubits.length,
    columns: model.componentGrid.length,
  });
  const ampMap = computeAmpMapForCircuit(
    model.qubits,
    model.componentGrid,
    endianness,
  );
  return prepareStateVizColumnsFromAmpMap(ampMap as any, opts);
}
