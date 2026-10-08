// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { toCircuitGroup } from "./circuit-vis/data/circuit.js";
import type { Circuit, CircuitGroup } from "./circuit-vis/data/circuit.js";
import { createStandaloneCircuitSvg } from "./circuit-vis/renderer/circuitSvgDocument.js";
import { renderCircuit } from "./circuit-vis/renderer/circuitRenderer.js";
import {
  type CircuitSvgRenderOptions,
  validateCircuitSvgRenderOptions,
} from "./circuit-vis/renderer/circuitSvgOptions.js";

export type {
  CircuitSvgBackground,
  CircuitSvgExpansion,
  CircuitSvgFontMode,
  CircuitSvgRenderOptions,
} from "./circuit-vis/renderer/circuitSvgOptions.js";

/**
 * Render QDK circuit data as a deterministic standalone SVG document.
 *
 * This uses the same preparation, layout, and gate formatters as the
 * interactive circuit visualizer. It does not mount a circuit or inspect host
 * DOM, CSS, fonts, or transient editor controls.
 */
export function renderCircuitSvg(
  circuit: Circuit | CircuitGroup,
  options: CircuitSvgRenderOptions = {},
): string {
  validateCircuitSvgRenderOptions(options);
  const result = toCircuitGroup(circuit);
  if (!result.ok) {
    throw new Error(result.error);
  }

  const circuitIndex = options.circuitIndex ?? 0;
  if (circuitIndex >= result.circuitGroup.circuits.length) {
    throw new Error(
      `Circuit index ${circuitIndex} is outside the available range 0-${Math.max(
        result.circuitGroup.circuits.length - 1,
        0,
      )}.`,
    );
  }

  const renderDepth = options.renderDepth ?? 0;
  const rendered = renderCircuit(result.circuitGroup.circuits[circuitIndex], {
    renderDepth,
    expansion: options.expansion ?? "model",
  });
  return createStandaloneCircuitSvg(rendered.svg, options);
}
