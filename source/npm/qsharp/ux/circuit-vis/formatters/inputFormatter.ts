// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { Column, ComponentGrid, Qubit, SourceLocation } from "../circuit.js";
import { RegisterType, RegisterMap, RegisterRenderData } from "../register.js";
import {
  leftPadding,
  startY,
  registerHeight,
  classicalRegHeight,
  nestedGroupPaddingTop,
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
  componentGrid: ComponentGrid,
  renderLocations?: (s: SourceLocation[]) => { title: string; href: string },
): { qubitWires: SVGElement; registers: RegisterMap; svgHeight: number } => {
  const qubitWires: SVGElement[] = [];
  const registers: RegisterMap = {};

  let currY: number = startY;
  qubits.forEach(({ id, numResults, declarations }, wireIndex) => {
    const topBorders = maxGroupTopBordersOnQubitRow(id, componentGrid);
    const topY = currY;
    currY += topBorders * nestedGroupPaddingTop;

    const link: { link?: { href: string; title: string } } = {};
    if (renderLocations && declarations && declarations.length > 0) {
      link.link = renderLocations(declarations);
    }

    // Add qubit wire to list of qubit wires
    qubitWires.push(qubitInput(currY, wireIndex, id.toString(), link.link));

    // Create qubit register
    registers[id] = { type: RegisterType.Qubit, wireY: currY, topY };

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
        topY: currY - classicalRegHeight + registerHeight / 2,
        wireY: currY,
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

function maxGroupTopBordersOnQubitRow(
  qubitIndex: number,
  componentGrid: ComponentGrid,
): number {
  let maxHeight = 0;
  for (const col of componentGrid) {
    maxHeight = Math.max(groupTopBordersOnQubitRow(qubitIndex, col), maxHeight);
  }
  return maxHeight;
}

function groupTopBordersOnQubitRow(qubitIndex: number, column: Column): number {
  let maxHeight = 0;
  for (const component of column.components) {
    if (component.dataAttributes?.["expanded"] === "true") {
      let qubits;
      switch (component.kind) {
        case "ket":
          qubits = component.targets;
          break;
        case "measurement":
          qubits = component.qubits;
          break;
        case "unitary":
          qubits = component.targets.concat(component.controls || []);
          break;
      }

      const minQubit = qubits
        .map((r) => r.qubit)
        .reduce((a, b) => Math.min(a, b));

      if (minQubit === qubitIndex) {
        const height =
          1 +
          maxGroupTopBordersOnQubitRow(qubitIndex, component.children || []);
        return height;
      } else {
        maxHeight = Math.max(
          maxHeight,
          maxGroupTopBordersOnQubitRow(qubitIndex, component.children || []),
        );
      }
    }
  }
  return maxHeight;
}

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
