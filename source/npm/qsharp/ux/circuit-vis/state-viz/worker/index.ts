// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Worker-safe exports for state visualization.
// Intentionally avoids importing UI modules (DOM/CSS).

export {
  computeAmpMapForCircuit,
  UnsupportedStateComputeError,
  type AmpMap,
} from "./stateCompute.js";

export {
  prepareStateVizColumnsFromAmpMap,
  type PrepareStateVizOptions,
} from "./stateVizPrep.js";
