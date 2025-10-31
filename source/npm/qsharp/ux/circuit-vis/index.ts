// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { Sqore } from "./sqore.js";
import { CircuitGroup } from "./circuit.js";

/**
 * Render `circuit` into `container` at the specified layer depth.
 *
 * @param circuitGroup Group of circuits to be visualized.
 * @param container HTML element for rendering visualization into.
 * @param renderDepth Initial layer depth at which to render gates.
 * @param isEditable Whether the circuit is editable.
 * @param editCallback Callback function to be called when the circuit is edited.
 * @param runCallback Callback function to be called when the circuit is run.
 */
export const draw = (
  circuitGroup: CircuitGroup,
  container: HTMLElement,
  renderDepth = 0,
  isEditable = false,
  editCallback?: (circuitGroup: CircuitGroup) => void,
  runCallback?: () => void,
): void => {
  const sqore = new Sqore(circuitGroup, isEditable, editCallback, runCallback);
  sqore.draw(container, renderDepth);
};

// Export types
export type {
  CircuitGroup,
  Circuit,
  ComponentGrid,
  Column,
  Qubit,
  Operation,
} from "./circuit.js";
