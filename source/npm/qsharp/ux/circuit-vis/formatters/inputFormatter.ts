// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { Qubit, SourceLocation } from "../circuit.js";
import { RegisterType, RegisterMap, RegisterRenderData } from "../register.js";
import {
  leftPadding,
  startY,
  classicalRegHeight,
  groupTopPadding,
  groupBottomPadding,
  gateHeight,
  gatePadding,
} from "../constants.js";
import { createSvgElement, group, text } from "./formatUtils.js";
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
  rowHeights: {
    [qubitIndex: number]: {
      heightAboveWire: number;
      heightBelowWire: number;
    };
  },
  renderLocations?: (s: SourceLocation[]) => { title: string; href: string },
): { qubitLabels: SVGElement; registers: RegisterMap; svgHeight: number } => {
  const qubitLabels: SVGElement[] = [];
  const registers: RegisterMap = {};

  let currY: number = startY;

  // currY ->    ┌╌╌╌╌╌╌╌┐
  //             ╎┌╌╌╌╌╌┐╎
  //             ╎╎     ╎╎
  //             ╎╎ ┌─┐ ╎╎
  //          ───┼┼─│X│─┼┼──
  //             ╎╎ └╥┘ ╎╎
  //             ╎╎  ╚══╪╪══
  //             ╎╎     ╎╎
  //             ╎└╌╌╌╌╌┘╎
  //             └╌╌╌╌╌╌╌┘

  qubits.forEach(({ id, numResults, declarations }, wireIndex) => {
    const { heightAboveWire, heightBelowWire } = rowHeights[wireIndex] || {
      heightAboveWire: 0,
      heightBelowWire: 0,
    };
    currY += heightAboveWire * groupTopPadding;

    //             ┌╌╌╌╌╌╌╌┐
    //             ╎┌╌╌╌╌╌┐╎
    // currY ->    ╎╎     ╎╎
    //             ╎╎ ┌─┐ ╎╎
    //          ───┼┼─│X│─┼┼──
    //             ╎╎ └╥┘ ╎╎
    //             ╎╎  ╚══╪╪══
    //             ╎╎     ╎╎
    //             ╎└╌╌╌╌╌┘╎
    //             └╌╌╌╌╌╌╌┘

    currY += gatePadding + gateHeight / 2;

    //             ┌╌╌╌╌╌╌╌┐
    //             ╎┌╌╌╌╌╌┐╎
    //             ╎╎     ╎╎
    //             ╎╎ ┌─┐ ╎╎
    // currY -> ───┼┼─│X│─┼┼──
    //             ╎╎ └╥┘ ╎╎
    //             ╎╎  ╚══╪╪══
    //             ╎╎     ╎╎
    //             ╎└╌╌╌╌╌┘╎
    //             └╌╌╌╌╌╌╌┘

    const link: { link?: { href: string; title: string } } = {};
    if (renderLocations && declarations && declarations.length > 0) {
      link.link = renderLocations(declarations);
    }

    // Add qubit wire to list of qubit wires
    qubitLabels.push(qubitInput(currY, wireIndex, id.toString(), link.link));

    // Create qubit register
    registers[id] = {
      type: RegisterType.Qubit,
      y: currY,
    };

    currY += gateHeight / 2;

    //             ┌╌╌╌╌╌╌╌┐
    //             ╎┌╌╌╌╌╌┐╎
    //             ╎╎     ╎╎
    //             ╎╎ ┌─┐ ╎╎
    //          ───┼┼─│X│─┼┼──
    // currY ->    ╎╎ └╥┘ ╎╎
    //             ╎╎  ╚══╪╪══
    //             ╎╎     ╎╎
    //             ╎└╌╌╌╌╌┘╎
    //             └╌╌╌╌╌╌╌┘

    // Increment current height by classical register height for attached classical registers

    // Add classical wires
    registers[id].children = Array.from(Array(numResults), () => {
      currY += classicalRegHeight;

      //             ┌╌╌╌╌╌╌╌┐
      //             ╎┌╌╌╌╌╌┐╎
      //             ╎╎     ╎╎
      //             ╎╎ ┌─┐ ╎╎
      //          ───┼┼─│X│─┼┼──
      //             ╎╎ └╥┘ ╎╎
      // currY ->    ╎╎  ╚══╪╪══
      //             ╎╎     ╎╎
      //             ╎└╌╌╌╌╌┘╎
      //             └╌╌╌╌╌╌╌┘

      const clsReg: RegisterRenderData = {
        type: RegisterType.Classical,
        y: currY,
      };
      return clsReg;
    });

    currY += gatePadding;

    //             ┌╌╌╌╌╌╌╌┐
    //             ╎┌╌╌╌╌╌┐╎
    //             ╎╎     ╎╎
    //             ╎╎ ┌─┐ ╎╎
    //          ───┼┼─│X│─┼┼──
    //             ╎╎ └╥┘ ╎╎
    //             ╎╎  ╚══╪╪══
    // currY ->    ╎╎     ╎╎
    //             ╎└╌╌╌╌╌┘╎
    //             └╌╌╌╌╌╌╌┘

    currY += heightBelowWire * groupBottomPadding;

    //             ┌╌╌╌╌╌╌╌┐
    //             ╎┌╌╌╌╌╌┐╎
    //             ╎╎     ╎╎
    //             ╎╎ ┌─┐ ╎╎
    //          ───┼┼─│X│─┼┼──
    //             ╎╎ └╥┘ ╎╎
    //             ╎╎  ║  ╎╎
    //             ╎╎  ╚══╪╪══
    //             ╎└╌╌╌╌╌┘╎
    //             └╌╌╌╌╌╌╌┘
    // currY ->
  });

  // Additional padding at the very bottom
  currY += classicalRegHeight;

  return {
    qubitLabels: group(qubitLabels, { class: "qubit-input-states" }),
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
  link?: { href: string; title: string },
): SVGElement => {
  const el: SVGElement = text("", leftPadding, y, 16);

  const subtext = subscript
    ? `<tspan baseline-shift="sub" font-size="65%">${subscript}</tspan>`
    : "";

  el.innerHTML = `|<tspan class="qs-mathtext">${mathChars.psi}</tspan>${subtext}${mathChars.rangle}</tspan>`;

  el.setAttribute("text-anchor", "start");
  el.setAttribute("dominant-baseline", "middle");
  el.setAttribute("data-wire", wireIndex.toString());
  el.classList.add("qs-maintext", "qs-qubit-label");

  if (link) {
    const linkElem = createSvgElement("a", {
      href: link.href,
      class: "qs-circuit-source-link",
    });

    // Add title as a child <title> element for accessibility and hover tooltip
    const titleElem = createSvgElement("title");
    titleElem.textContent = link.title;
    linkElem.appendChild(titleElem);

    // Add the gate as a child of the link
    linkElem.appendChild(el);
    return linkElem;
  }
  return el;
};

export { formatInputs, qubitInput };
