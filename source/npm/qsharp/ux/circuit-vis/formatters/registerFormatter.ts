// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { RegisterMap } from "../register.js";
import {
  regLineStart,
  classicalStubLength,
  gateHeight,
  controlCircleRadius,
} from "../constants.js";
import { GateRenderData, GateType } from "../gateRenderData.js";
import { ClassicalWireLayout } from "../classicalWireAnalysis.js";
import { group, line } from "./formatUtils.js";

/**
 * Generate the SVG representation of the qubit register wires in `registers` and the classical wires
 * stemming from each measurement gate.
 *
 * Unused results (not feeding a classically-controlled gate) render as a short
 * vertical stub below the measurement box.  Used results extend horizontally
 * only to the rightmost consuming ClassicalControlled gate.
 *
 * @param registers           Map from register IDs to register render data.
 * @param allGates            All the gates in the circuit.
 * @param endX                End x-coordinate for the whole circuit.
 * @param classicalWireLayout Layout information for classical wires.
 *
 * @returns SVG representation of register wires.
 */
const formatRegisters = (
  registers: RegisterMap,
  allGates: GateRenderData[],
  endX: number,
  classicalWireLayout: ClassicalWireLayout,
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
    const children = registers[qId].children || [];
    for (let resultIdx = 0; resultIdx < children.length; resultIdx++) {
      const classical = children[resultIdx];
      const wireKey = `${qId}-${resultIdx}`;
      const wireInfo = classicalWireLayout.wireInfos.get(wireKey);
      const isUsed = wireInfo?.isUsedAsControl ?? false;

      if (!isUsed) {
        // Unused result: short vertical stub only.
        // Find the measurement gate that produces this result by wire key.
        for (const gate of allGates.flat()) {
          if (gate.type === GateType.Measure && gate.resultWireKey === wireKey) {
            const qubitY = gate.controlsY[0];
            // Start the stub at the bottom edge of the gate box,
            // not at the qubit wire (which is the gate center).
            const gateBoxBottom = qubitY + gateHeight / 2;
            classicalRegs.push(
              _classicalStub(
                gate.x,
                gateBoxBottom,
                gateBoxBottom + classicalStubLength,
              ),
            );
            break;
          }
        }
      } else {
        // Used result: vertical connector + horizontal wire to the rightmost
        // ClassicalControlled gate's control circle x.
        // Find the measurement gate by wire key for the startX, and find the
        // consuming gate for the endX.
        const wireRange = classicalWireLayout.wireRanges.get(wireKey);

        for (const gate of allGates.flat()) {
          if (gate.type !== GateType.Measure || gate.resultWireKey !== wireKey) {
            continue;
          }

          // Found the measurement gate that produces this wire.
          const verticalY = gate.controlsY[0];

          // Determine the end x for this wire.
          // Scan allGates for ClassicalControlled gates that consume
          // this specific wire key — use the rightmost one's control circle x.
          let wireEndX = endX;
          if (wireRange) {
            let maxCtrlX = gate.x;
            for (const ctrlGate of allGates.flat()) {
              if (ctrlGate.type === GateType.ClassicalControlled) {
                if (ctrlGate.controlWireKeys?.includes(wireKey)) {
                  // The control circle is at the left edge of the gate
                  // bounding box + controlCircleRadius.
                  const circleX =
                    ctrlGate.x -
                    ctrlGate.width / 2 +
                    controlCircleRadius;
                  maxCtrlX = Math.max(maxCtrlX, circleX);
                }
              }
            }
            wireEndX = maxCtrlX;
          }

          classicalRegs.push(
            _classicalRegister(gate.x, wireEndX, classical.y, verticalY),
          );
          break;
        }
      }
    }
  }

  return group(qubitRegs.concat(classicalRegs), { class: "wires" });
};

/**
 * Generates the SVG representation of a short vertical classical stub
 * (for measurement results that are not used as controls).
 *
 * @param x      x coord of the measurement gate center.
 * @param startY y coord of the qubit wire.
 * @param endY   y coord of the stub bottom.
 *
 * @returns SVG group with the stub lines.
 */
const _classicalStub = (
  x: number,
  startY: number,
  endY: number,
): SVGElement => {
  const wirePadding = 1;
  const vLine1: SVGElement = line(
    x + wirePadding,
    startY,
    x + wirePadding,
    endY,
    "register-classical",
  );
  const vLine2: SVGElement = line(
    x - wirePadding,
    startY,
    x - wirePadding,
    endY,
    "register-classical",
  );
  return group([vLine1, vLine2]);
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
