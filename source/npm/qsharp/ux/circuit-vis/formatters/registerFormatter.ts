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
 * @param endX         End x-coordinate for the whole circuit.
 *                     All wires will stretch to this x-coordinate.
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
        registers[qId].wireY,
        endX,
        registers[qId].wireY,
        "qubit-wire",
      ),
    );

    // Render classical wires
    for (const classical of registers[qId].children || []) {
      for (const gate of allGates.flat()) {
        if (
          gate.type === GateType.Group &&
          gate.dataAttributes?.["expanded"] === "true"
        ) {
          // Don't render classical wires for a group that is expanded - the wires
          // will be coming out of the measurement operations *inside* the group.
          continue;
        }

        for (const y of gate.targetsY
          .flat()
          .filter((y) => y === classical.wireY)) {
          // Found the gate that this classical wire originates from. Draw
          // it starting at this gates x-coordinate.

          // If this is a measurement gate, there is a vertical line
          // going down from the gate to the wire
          const verticalY =
            gate.type === GateType.Measure ? gate.controlsY[0] : undefined;

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
 * @param endX   End x coord.
 * @param wireY  y coord of wire.
 * @param gateY  y coord of the measurement gate that this wire originates from.
 *               If undefined, no vertical line is drawn.
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
