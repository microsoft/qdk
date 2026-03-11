// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { Qubit, SourceLocation } from "../circuit.js";
import { RegisterType, RegisterMap, RegisterRenderData } from "../register.js";
import {
  leftPadding,
  startY,
  classicalRegHeight,
  classicalStubLength,
  groupTopPadding,
  groupBottomPadding,
  gateHeight,
  gatePadding,
} from "../constants.js";
import { ClassicalWireLayout } from "../classicalWireAnalysis.js";
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
  classicalWireLayout: ClassicalWireLayout,
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
  //             ╎╎  ║  ╎╎
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
    //             ╎╎  ║  ╎╎
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
    //             ╎╎  ║  ╎╎
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

    currY += gatePadding + gateHeight / 2;

    //             ┌╌╌╌╌╌╌╌┐
    //             ╎┌╌╌╌╌╌┐╎
    //             ╎╎     ╎╎
    //             ╎╎ ┌─┐ ╎╎
    //          ───┼┼─│X│─┼┼──
    //             ╎╎ └╥┘ ╎╎
    // currY ->    ╎╎  ║  ╎╎
    //             ╎╎  ╚══╪╪══
    //             ╎╎     ╎╎
    //             ╎└╌╌╌╌╌┘╎
    //             └╌╌╌╌╌╌╌┘

    // Add classical wires
    // Slot-based allocation: used results get dedicated vertical slots,
    // unused results share a short stub position just below the gate box.
    const numSlots = classicalWireLayout.maxSlots.get(id) ?? 0;
    // stubY sits at the tip of the visual stub, which starts at gateBoxBottom
    // (= qubitY + gateHeight/2 = currY - gatePadding) and extends classicalStubLength below it.
    const gateBoxBottom = currY - gatePadding;
    const stubY = gateBoxBottom + classicalStubLength;

    registers[id].children = Array.from(
      Array(numResults ?? 0),
      (_, resultIdx) => {
        const key = `${id}-${resultIdx}`;
        const slot = classicalWireLayout.slotAssignment.get(key);
        let regY: number;
        if (slot != null) {
          // Used result: dedicated y position based on slot index.
          regY = currY + (slot + 1) * (gateHeight / 2 + gatePadding);
        } else {
          // Unused result: shared stub position.
          regY = stubY;
        }
        const clsReg: RegisterRenderData = {
          type: RegisterType.Classical,
          y: regY,
        };
        return clsReg;
      },
    );

    // Advance currY: used slots get full-height vertical space,
    // unused stubs still need enough room so they don't overlap the next qubit.
    if (numSlots > 0) {
      currY += numSlots * (gateHeight / 2 + gatePadding);
      // Add space below the last result wire so the enclosing dashed box
      // (which extends gateHeight/2 + groupBottomPadding below the wire)
      // does not overlap the next sibling group's dashed box.
      currY += gateHeight / 2 + gatePadding;
    } else if ((numResults ?? 0) > 0) {
      currY += classicalStubLength;
    }

    //             ┌╌╌╌╌╌╌╌┐
    //             ╎┌╌╌╌╌╌┐╎
    //             ╎╎     ╎╎
    //             ╎╎ ┌─┐ ╎╎
    //          ───┼┼─│X│─┼┼──
    //             ╎╎ └╥┘ ╎╎
    //             ╎╎  ║  ╎╎
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
    //             ╎╎     ╎╎
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
