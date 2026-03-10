// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { DrawOptions, Sqore } from "./sqore.js";
import { CircuitGroup } from "./circuit.js";

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
 *       - `computeStateVizColumnsForCircuitModel` (optional): When provided,
 *         delegates async state visualization computation to the host, which
 *         is necessary for large circuits and/or when using a Web Worker (e.g. in VS Code).
 *         When omitted, state visualization will be computed on the main thread.
 */
export const draw = (
  circuitGroup: CircuitGroup,
  container: HTMLElement,
  options: DrawOptions = {},
): {
  userSetZoomLevel: (zoomLevel: number) => void;
} => {
  const sqore = new Sqore(circuitGroup, options);
  sqore.draw(container);
  return {
    userSetZoomLevel: (zoomLevel: number) => {
      sqore.zoomOnResize = false;
      sqore.updateZoomLevel(zoomLevel);
    },
  };
};

/**
 * Export a circuit visualization as a standalone SVG string.
 *
 * The returned string is a fully self-contained SVG document with inlined
 * styles; it can be written directly to a `.svg` file or embedded in HTML
 * without requiring any external stylesheet or VS Code theme variables.
 *
 * @param circuitGroup Group of circuits to be visualized.
 * @param options Optional rendering options (same subset as `draw`).
 * @returns SVG markup string starting with an XML declaration.
 */
export const exportToSvg = (
  circuitGroup: CircuitGroup,
  options: Pick<DrawOptions, "renderDepth" | "renderLocations"> = {},
): string => {
  const sqore = new Sqore(circuitGroup, options);
  return sqore.toSvgString();
};

export type { DrawOptions, EditorHandlers } from "./sqore.js";

// Export types
export type {
  CircuitGroup,
  Circuit,
  ComponentGrid,
  Column,
  Qubit,
  Operation,
} from "./circuit.js";
