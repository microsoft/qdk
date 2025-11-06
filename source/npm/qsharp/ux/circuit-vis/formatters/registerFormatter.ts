// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { RegisterMap } from "../register.js";
import { regLineStart } from "../constants.js";
import { GateRenderData, GateType } from "../gateRenderData.js";
import { group, line } from "./formatUtils.js";

/**
 * Generate the SVG representation of the qubit register wires in `registers` and the classical wires
 * stemming from each measurement gate.
 *
 * @param registers    Map from register IDs to register render data.
 * @param measureGates Array of measurement gate render data.
 * @param endX         End x coord.
 *
 * @returns SVG representation of register wires.
 */
const formatRegisters = (
  registers: RegisterMap,
  measureGates: GateRenderData[],
  endX: number,
): SVGElement => {
  const formattedRegs: SVGElement[] = [];
  // Render qubit wires
  for (const qId in registers) {
    formattedRegs.push(
      line(
        regLineStart,
        registers[qId].y,
        endX,
        registers[qId].y,
        "qubit-wire",
      ),
    );
  }
  // Render classical wires
  measureGates.forEach(({ type, x, targetsY, controlsY }) => {
    if (type !== GateType.Measure) return;
    const gateY: number = controlsY[0];
    (targetsY as number[]).forEach((y) => {
      formattedRegs.push(_classicalRegister(x, gateY, endX, y));
    });
  });
  return group(formattedRegs, { class: "wires" });
};

/**
 * Generates the SVG representation of a classical register.
 *
 * @param startX Start x coord.
 * @param gateY  y coord of measurement gate.
 * @param endX   End x coord.
 * @param wireY  y coord of wire.
 *
 * @returns SVG representation of the given classical register.
 */
const _classicalRegister = (
  startX: number,
  gateY: number,
  endX: number,
  wireY: number,
): SVGElement => {
  const wirePadding = 1;
  // Draw vertical lines
  const vLine1: SVGElement = line(
    startX + wirePadding,
    gateY,
    startX + wirePadding,
    wireY - wirePadding,
    "register-classical",
  );
  const vLine2: SVGElement = line(
    startX - wirePadding,
    gateY,
    startX - wirePadding,
    wireY + wirePadding,
    "register-classical",
  );

  // Draw horizontal lines
  const hLine1: SVGElement = line(
    startX + wirePadding,
    wireY - wirePadding,
    endX,
    wireY - wirePadding,
    "register-classical",
  );
  const hLine2: SVGElement = line(
    startX - wirePadding,
    wireY + wirePadding,
    endX,
    wireY + wirePadding,
    "register-classical",
  );

  return group([vLine1, vLine2, hLine1, hLine2]);
};

export { formatRegisters };
