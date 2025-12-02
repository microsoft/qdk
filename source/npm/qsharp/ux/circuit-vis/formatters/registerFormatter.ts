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
 * @param allGates     All the gates in the circuit.
 * @param endX         End x coord.
 *
 * @returns SVG representation of register wires.
 */
const formatRegisters = (
  registers: RegisterMap,
  allGates: GateRenderData[],
  endX: number,
): SVGElement => {
  const qubitRegs: SVGElement[] = [];
  const classicalRegs: SVGElement[] = [];
  for (const qId in registers) {
    // Render qubit wire
    qubitRegs.push(
      line(
        regLineStart,
        registers[qId].y,
        endX,
        registers[qId].y,
        "qubit-wire",
      ),
    );

    // Render classical wires
    for (const classical of registers[qId].children || []) {
      for (const gate of allGates.flat()) {
        if (gate.dataAttributes?.["expanded"] === "true") {
          continue;
        }

        const verticalY =
          gate.type === GateType.Measure ? gate.controlsY[0] : undefined;

        for (const y of gate.targetsY.flat().filter((y) => y === classical.y)) {
          // Found the gate that writes to this classical register
          classicalRegs.push(_classicalRegister(gate.x, endX, y, verticalY));
        }
      }
    }
  }

  return group(qubitRegs.concat(classicalRegs), { class: "wires" });
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
  endX: number,
  wireY: number,
  gateY?: number,
): SVGElement => {
  const wirePadding = 1;
  const g = [];
  if (gateY != null) {
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
    g.push(vLine1, vLine2);
  }

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

  g.push(hLine1, hLine2);

  return group(g);
};

export { formatRegisters };
