// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { Qubit } from "../circuit.js";
import { RegisterType, RegisterMap, RegisterRenderData } from "../register.js";
import {
  leftPadding,
  startY,
  registerHeight,
  classicalRegHeight,
} from "../constants.js";
import { group, text } from "./formatUtils.js";
import { mathChars } from "../utils.js";

/**
 * `formatInputs` takes in an array of Qubits and outputs the SVG string of formatted
 * qubit wires and a mapping from register IDs to register rendering data.
 *
 * @param qubits List of declared qubits.
 *
 * @returns returns the SVG string of formatted qubit wires, a mapping from registers
 *          to y coord and total SVG height.
 */
const formatInputs = (
  qubits: Qubit[],
): { qubitWires: SVGElement; registers: RegisterMap; svgHeight: number } => {
  const qubitWires: SVGElement[] = [];
  const registers: RegisterMap = {};

  let currY: number = startY;
  qubits.forEach(({ id, numResults }, wireIndex) => {
    // Add qubit wire to list of qubit wires
    qubitWires.push(qubitInput(currY, wireIndex, id.toString()));

    // Create qubit register
    registers[id] = { type: RegisterType.Qubit, y: currY };

    // If there are no attached classical registers, increment y by fixed register height
    if (numResults == null || numResults === 0) {
      currY += registerHeight;
      return;
    }

    // Increment current height by classical register height for attached classical registers
    currY += classicalRegHeight;

    // Add classical wires
    registers[id].children = Array.from(Array(numResults), () => {
      const clsReg: RegisterRenderData = {
        type: RegisterType.Classical,
        y: currY,
      };
      currY += classicalRegHeight;
      return clsReg;
    });
  });

  return {
    qubitWires: group(qubitWires, { class: "qubit-input-states" }),
    registers,
    svgHeight: currY,
  };
};

/**
 * Generate the SVG text component for the input qubit register.
 *
 * @param y y coord of input wire to render in SVG.
 *
 * @returns SVG text component for the input register.
 */
const qubitInput = (
  y: number,
  wireIndex: number,
  subscript?: string,
): SVGElement => {
  const el: SVGElement = text("", leftPadding, y, 16);

  const subtext = subscript
    ? `<tspan baseline-shift="sub" font-size="65%">${subscript}</tspan>`
    : "";

  el.innerHTML = `|<tspan class="qs-mathtext">${mathChars.psi}</tspan>${subtext}${mathChars.rangle}</tspan>`;

  el.setAttribute("text-anchor", "start");
  el.setAttribute("dominant-baseline", "middle");
  el.setAttribute("data-wire", wireIndex.toString());
  el.classList.add("qs-maintext");
  return el;
};

export { formatInputs, qubitInput };
