// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { Operation } from "../data/circuit.js";
import { gateHeight, minGateWidth } from "../renderer/constants.js";
import { GateRenderData, GateType } from "../renderer/gateRenderData.js";
import { getMinGateWidth } from "../utils.js";

/**
 * Build a `GateRenderData` for a single operation rendered at (`x`, `y`) without consulting real
 * register positions.
 *
 * The main render path (see [process.ts](../renderer/process.ts)) derives gate geometry from the
 * surrounding circuit. The editor needs to render gates outside that context — toolbox icons and
 * the drag ghost in [draggable.ts](draggable.ts) — so this helper fakes a single wire centered in
 * the gate body. Width still goes through `getMinGateWidth`, so a toolbox icon matches the same
 * gate dropped onto the circuit.
 *
 * Limited gate-kind support — only what the toolbox + drag ghost use.
 *
 * @param operation     Operation to render. `undefined` returns an `Invalid`-typed render data the
 *   caller can treat as a placeholder.
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
