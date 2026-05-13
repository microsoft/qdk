// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { Operation } from "../data/circuit.js";
import { gateHeight, minGateWidth } from "../renderer/constants.js";
import { GateRenderData, GateType } from "../renderer/gateRenderData.js";
import { getMinGateWidth } from "../utils.js";

/**
 * Build a `GateRenderData` for a single operation rendered at
 * (`x`, `y`) without consulting real register positions.
 *
 * The main render path (see [process.ts](../renderer/process.ts))
 * computes a gate's geometry from the surrounding circuit's
 * registers — qubit Y coords, classical wire splits, neighboring
 * column widths. The editor needs to render gates *outside* that
 * context for two cases:
 *
 * - the toolbox, where each palette item is a sized icon sitting in
 *   its own little SVG; and
 * - the drag ghost in [draggable.ts](draggable.ts), where the gate
 *   being dragged floats at the cursor before it's dropped.
 *
 * Neither has a real register layout to consult, so this helper
 * fakes one: a single wire centered in the gate body. Gate width
 * still goes through `getMinGateWidth`, so a toolbox icon comes out
 * the same width as the same gate dropped onto the circuit.
 *
 * Limited gate-kind support — only what the toolbox + drag ghost
 * actually use today.
 *
 * @param operation     Operation to render. `undefined` returns an
 *                      `Invalid`-typed render data the caller can
 *                      treat as a placeholder.
 * @param x             x coordinate at the gate's top-left.
 * @param y             y coordinate at the gate's top-left.
 * @returns             GateRenderData object.
 */
const toRenderData = (
  operation: Operation | undefined,
  x: number,
  y: number,
): GateRenderData => {
  const target = y + 1 + gateHeight / 2; // offset by 1 for top padding
  const renderData: GateRenderData = {
    type: GateType.Invalid,
    isExpanded: false,
    x: x + 1 + minGateWidth / 2, // offset by 1 for left padding
    controlsY: [],
    targetsY: [target],
    label: "",
    width: -1,
    topPadding: 0,
    bottomPadding: 0,
  };

  if (operation === undefined) return renderData;

  switch (operation.kind) {
    case "unitary": {
      const { gate, controls } = operation;

      if (gate === "SWAP") {
        renderData.type = GateType.Swap;
      } else if (controls && controls.length > 0) {
        renderData.type =
          gate === "X" ? GateType.Cnot : GateType.ControlledUnitary;
        renderData.label = gate;
        if (gate !== "X") {
          renderData.targetsY = [[target]];
        }
      } else if (gate === "X") {
        renderData.type = GateType.X;
        renderData.label = gate;
      } else {
        renderData.type = GateType.Unitary;
        renderData.label = gate;
        renderData.targetsY = [[target]];
      }
      break;
    }
    case "measurement":
      renderData.type = GateType.Measure;
      renderData.controlsY = [target];
      break;
    case "ket":
      renderData.type = GateType.Ket;
      renderData.label = operation.gate;
      renderData.targetsY = [[target]];
      break;
  }

  if (operation.args !== undefined && operation.args.length > 0)
    renderData.displayArgs = operation.args[0];

  renderData.width = getMinGateWidth(renderData);
  renderData.x = x + 1 + renderData.width / 2; // offset by 1 for left padding

  return renderData;
};

export { toRenderData };
