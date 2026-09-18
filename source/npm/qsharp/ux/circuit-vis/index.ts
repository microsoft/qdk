// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { DrawOptions, Sqore } from "./sqore.js";
import { CircuitGroup } from "./data/circuit.js";
import type { CircuitRendererSvgOptions } from "./renderer/circuitSvgOptions.js";

export type CircuitRenderer = {
  userSetZoomLevel: (zoomLevel: number) => void;
  /**
   * Replace the rendered circuit in place, preserving per-session view state.
   */
  updateCircuit: (circuitGroup: CircuitGroup) => void;
  /**
   * Serialize the exact circuit owned by this renderer as a standalone SVG.
   */
  exportSvg: (options?: CircuitRendererSvgOptions) => string;
};

/**
 * Render `circuit` into `container` at the specified layer depth.
 *
 * @param circuitGroup Group of circuits to be visualized.
 * @param container HTML element for rendering visualization into.
 * @param options Rendering/interaction options.
 *   - `renderDepth`: Initial layer depth at which to render gates.
 *   - `renderLocations`: Callback to generate links for source locations.
 *   - `editor`: When provided, enables editing behaviors and requires:
 *       - `editCallback`: Called when the circuit is edited.
 *       - `runCallback` (optional): When provided, enables the Run button.
 *       - `computeStateVizColumnsForCircuitModel` (optional): When provided, delegates async state
 *         visualization computation to the host, which is necessary for large circuits and/or when
 *         using a Web Worker (e.g. in VS Code). When omitted, state visualization will be computed
 *         on the main thread.
 */
export const draw = (
  circuitGroup: CircuitGroup,
  container: HTMLElement,
  options: DrawOptions = {},
): CircuitRenderer => {
  const sqore = new Sqore(circuitGroup, options);
  sqore.draw(container);
  return {
    userSetZoomLevel: (zoomLevel: number) => {
      sqore.zoomOnResize = false;
      sqore.updateZoomLevel(zoomLevel);
    },
    updateCircuit: (group: CircuitGroup) => sqore.updateCircuit(group),
    exportSvg: (exportOptions) => sqore.exportSvg(exportOptions),
  };
};

export type { DrawOptions, EditorHandlers } from "./sqore.js";
export type { CircuitRendererSvgOptions } from "./renderer/circuitSvgOptions.js";

// Export types
export type {
  CircuitGroup,
  Circuit,
  ComponentGrid,
  Column,
  Qubit,
  Operation,
} from "./data/circuit.js";
