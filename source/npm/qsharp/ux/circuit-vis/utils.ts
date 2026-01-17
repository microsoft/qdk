// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { GateRenderData, GateType } from "./gateRenderData.js";
import {
  minGateWidth,
  labelPaddingX,
  labelFontSize,
  argsFontSize,
} from "./constants.js";
import { ComponentGrid, Operation } from "./circuit.js";
import { Register } from "./register.js";

/**
 * Performs a deep equality check between two objects or arrays.
 * @param obj1 - The first object or array to compare.
 * @param obj2 - The second object or array to compare.
 * @returns True if the objects are deeply equal, false otherwise.
 */
const deepEqual = (obj1: unknown, obj2: unknown): boolean => {
  if (obj1 === obj2) return true;

  if (
    obj1 === null ||
    obj2 === null ||
    typeof obj1 !== "object" ||
    typeof obj2 !== "object"
  ) {
    return false;
  }

  const keys1 = Object.keys(obj1);
  const keys2 = Object.keys(obj2);

  if (keys1.length !== keys2.length) return false;

  for (const key of keys1) {
    if (
      !keys2.includes(key) ||
      !deepEqual(
        (obj1 as Record<string, unknown>)[key],
        (obj2 as Record<string, unknown>)[key],
      )
    ) {
      return false;
    }
  }

  return true;
};

/**
 * Calculate the width of a gate, given its render data.
 *
 * @param renderData - The rendering data of the gate, including its type, label, display arguments.
 *
 * @returns Width of given gate (in pixels).
 */
const getMinGateWidth = ({
  type,
  label,
  displayArgs,
}: GateRenderData): number => {
  switch (type) {
    case GateType.Measure:
    case GateType.Cnot:
    case GateType.Swap:
      return minGateWidth;
    default: {
      const labelWidth = _getStringWidth(label);
      const argsWidth =
        displayArgs != null ? _getStringWidth(displayArgs, argsFontSize) : 0;
      const textWidth = Math.max(labelWidth, argsWidth) + labelPaddingX * 2;
      return Math.max(minGateWidth, textWidth);
    }
  }
};

/**
 * Estimate string width in pixels based on character types and font size.
 * This may not match the true rendered width, but should be close enough for
 * calculating layout.
 *
 * @param text - The text string to measure.
 * @param fontSize - The font size in pixels (default is labelFontSize).
 *
 * @returns Estimated width of the string in pixels.
 */
const _getStringWidth = (
  text: string,
  fontSize: number = labelFontSize,
): number => {
  let units = 0;
  for (const ch of Array.from(text)) {
    if (ch === " ") {
      units += 0.33;
      continue;
    }
    if ("il.:;,'`!|".includes(ch)) {
      units += 0.28;
      continue;
    }
    if ("mw".includes(ch)) {
      units += 0.72;
      continue;
    }
    if ("MW@#%&".includes(ch)) {
      units += 0.78;
      continue;
    }
    if (/[0-9]/.test(ch)) {
      units += 0.55;
      continue;
    }
    if (/[A-Z]/.test(ch)) {
      units += 0.56;
      continue;
    }
    if (/[a-z]/.test(ch)) {
      units += 0.5;
      continue;
    }
    if (/[θπ]/.test(ch)) {
      units += 0.56;
      continue;
    }
    if (/[ψ]/.test(ch)) {
      units += 0.6;
      continue;
    }
    if ("-+*/=^~_<>".includes(ch)) {
      units += 0.5;
      continue;
    }
    units += 0.56;
  }
  const kerningFudge = Math.max(0, text.length - 1) * 0.005;
  // Round to a whole number to keep it easy to read
  return Math.floor((units + kerningFudge) * fontSize);
};

/**
 * Find targets of an operation's children by recursively walking
 * through all of its children's controls and targets.
 * Note that this intentionally ignores the direct targets of the
 * operation itself.
 *
 * Example:
 * Gate Foo contains gate H and gate RX.
 * qIds of Gate H is 1
 * qIds of Gate RX are 1, 2
 * This should return [{qId: 1}, {qId: 2}]
 *
 * @param operation The operation to find targets for.
 * @returns An array of registers with unique qIds.
 */
const getChildTargets = (operation: Operation): Register[] | [] => {
  const _recurse = (operation: Operation) => {
    switch (operation.kind) {
      case "measurement":
        registers.push(...operation.qubits);
        registers.push(...operation.results);
        break;
      case "unitary":
        registers.push(...operation.targets);
        if (operation.controls) {
          registers.push(...operation.controls);
        }
        break;
      case "ket":
        registers.push(...operation.targets);
        break;
    }

    // If there is more children, keep adding more to registers
    if (operation.children) {
      operation.children.forEach((col) =>
        col.components.forEach((child) => {
          _recurse(child);
        }),
      );
    }
  };

  const registers: Register[] = [];
  if (operation.children == null) return [];

  // Recursively walkthrough all children to populate registers
  operation.children.forEach((col) =>
    col.components.forEach((child) => {
      _recurse(child);
    }),
  );

  // Extract qIds from array of object
  // i.e. [{qId: 0}, {qId: 1}, {qId: 1}] -> [0, 1, 1]
  const qIds = registers.map((register) => register.qubit);
  const uniqueQIds = Array.from(new Set(qIds));

  // Transform array of numbers into array of qId object
  // i.e. [0, 1] -> [{qId: 0}, {qId: 1}]
  return uniqueQIds.map((qId) => ({ qubit: qId }));
};

/**
 * Split a location string into an array of index tuples.
 *
 * Example:
 * "0,1-0,2-2,3" -> [[0,1], [0,2], [2,3]]
 *
 * @param location The location string to split.
 * @returns An array of indexes.
 */
const locationStringToIndexes = (location: string): [number, number][] => {
  return location !== ""
    ? location.split("-").map((segment) => {
        const coords = segment.split(",");
        if (coords.length !== 2) throw new Error("Invalid location");
        return [parseInt(coords[0]), parseInt(coords[1])];
      })
    : [];
};

/**
 * Gets the location of an operation, if it has one.
 *
 * @param operation The operation to get the location for.
 * @returns The location string of the operation, or null if it doesn't have one.
 */
const getGateLocationString = (operation: Operation): string | null => {
  if (operation.dataAttributes == null) return null;
  return operation.dataAttributes["location"];
};

/**
 * Get the minimum and maximum register indices for a given operation.
 *
 * @param operation The operation for which to get the register indices.
 * @param numQubits The number of qubits in the circuit.
 * @returns A tuple containing the minimum and maximum register indices.
 */
function getMinMaxRegIdx(
  operation: Operation,
  numQubits: number,
): [number, number] {
  let targets: Register[];
  let controls: Register[];
  switch (operation.kind) {
    case "measurement":
      targets = operation.results;
      controls = operation.qubits;
      break;
    case "unitary":
      targets = operation.targets;
      controls = operation.controls || [];
      break;
    case "ket":
      targets = operation.targets;
      controls = [];
      break;
  }

  const qRegs = [...controls, ...targets]
    .filter(({ result }) => result === undefined)
    .map(({ qubit }) => qubit);
  const clsControls: Register[] = controls.filter(
    ({ result }) => result !== undefined,
  );
  const isClassicallyControlled: boolean = clsControls.length > 0;
  if (!isClassicallyControlled && qRegs.length === 0) return [-1, -1];
  // If operation is classically-controlled, pad all qubit registers. Otherwise, only pad
  // the contiguous range of registers that it covers.
  const minRegIdx: number = isClassicallyControlled ? 0 : Math.min(...qRegs);
  const maxRegIdx: number = isClassicallyControlled
    ? numQubits - 1
    : Math.max(...qRegs);

  return [minRegIdx, maxRegIdx];
}

/**********************
 *  Finder Functions  *
 **********************/

/**
 * Find the surrounding gate element of a host element.
 *
 * @param hostElem The SVG element representing the host element.
 * @returns The surrounding gate element or null if not found.
 */
const findGateElem = (hostElem: SVGElement): SVGElement | null => {
  return hostElem.closest<SVGElement>("[data-location]");
};

/**
 * Find the location of the gate surrounding a host element.
 *
 * @param hostElem The SVG element representing the host element.
 * @returns The location string of the surrounding gate or null if not found.
 */
const findLocation = (hostElem: SVGElement) => {
  const gateElem = findGateElem(hostElem);
  return gateElem != null ? gateElem.getAttribute("data-location") : null;
};

/**
 * Find the parent operation of the operation specified by location.
 *
 * @param componentGrid The grid of components to search through.
 * @param location The location string of the operation.
 * @returns The parent operation or null if not found.
 */
const findParentOperation = (
  componentGrid: ComponentGrid,
  location: string | null,
): Operation | null => {
  if (!location) return null;

  const indexes = locationStringToIndexes(location);
  indexes.pop();
  const lastIndex = indexes.pop();

  if (lastIndex == null) return null;

  let parentOperation = componentGrid;
  for (const index of indexes) {
    parentOperation =
      parentOperation[index[0]].components[index[1]].children ||
      parentOperation;
  }
  return parentOperation[lastIndex[0]].components[lastIndex[1]];
};

/**
 * Find the parent component grid of an operation based on its location.
 *
 * @param componentGrid The grid of components to search through.
 * @param location The location string of the operation.
 * @returns The parent grid of components or null if not found.
 */
const findParentArray = (
  componentGrid: ComponentGrid,
  location: string | null,
): ComponentGrid | null => {
  if (!location) return null;

  const indexes = locationStringToIndexes(location);
  indexes.pop(); // The last index refers to the operation itself, remove it so that the last index instead refers to the parent operation

  let parentArray = componentGrid;
  for (const index of indexes) {
    parentArray =
      parentArray[index[0]].components[index[1]].children || parentArray;
  }
  return parentArray;
};

/**
 * Find an operation based on its location.
 *
 * @param componentGrid The grid of components to search through.
 * @param location The location string of the operation.
 * @returns The operation or null if not found.
 */
const findOperation = (
  componentGrid: ComponentGrid,
  location: string | null,
): Operation | null => {
  if (!location) return null;

  const index = locationStringToIndexes(location).pop();
  const operationParent = findParentArray(componentGrid, location);

  if (operationParent == null || index == null) return null;

  return operationParent[index[0]].components[index[1]];
};

/**********************
 *  Getter Functions  *
 **********************/

/**
 * Get list of y values based on circuit wires.
 *
 * @param container The HTML container element containing the circuit visualization.
 * @returns An array of y values corresponding to the circuit wires.
 */
const getWireData = (container: HTMLElement): number[] => {
  const wireElems = container.querySelectorAll<SVGGElement>(".qubit-wire");
  const wireData = Array.from(wireElems).map((wireElem) => {
    return Number(wireElem.getAttribute("y1"));
  });
  return wireData;
};

/**
 * Get list of toolbox items.
 *
 * @param container The HTML container element containing the toolbox items.
 * @returns An array of SVG graphics elements representing the toolbox items.
 */
const getToolboxElems = (container: HTMLElement): SVGGraphicsElement[] => {
  return Array.from(
    container.querySelectorAll<SVGGraphicsElement>("[toolbox-item]"),
  );
};

/**
 * Get list of host elements that dropzones can be attached to.
 *
 * @param container The HTML container element containing the circuit visualization.
 * @returns An array of SVG graphics elements representing the host elements.
 */
const getHostElems = (container: HTMLElement): SVGGraphicsElement[] => {
  const circuitSvg = container.querySelector("svg.qviz");
  return circuitSvg != null
    ? Array.from(
        circuitSvg.querySelectorAll<SVGGraphicsElement>(
          '[class^="gate-"]:not(.gate-control, .gate-swap), .control-dot, .oplus, .cross',
        ),
      )
    : [];
};

/**
 * Get list of gate elements from the circuit, but not the toolbox.
 *
 * @param container The HTML container element containing the circuit visualization.
 * @returns An array of SVG graphics elements representing the gate elements.
 */
const getGateElems = (container: HTMLElement): SVGGraphicsElement[] => {
  const circuitSvg = container.querySelector("svg.qviz");
  return circuitSvg != null
    ? Array.from(circuitSvg.querySelectorAll<SVGGraphicsElement>(".gate"))
    : [];
};

/**
 * Get list of qubit label elements for drag-and-drop.
 *
 * @param container The HTML container element containing the circuit visualization.
 * @returns An array of SVGTextElement representing the qubit labels.
 */
const getQubitLabelElems = (container: HTMLElement): SVGTextElement[] => {
  const circuitSvg = container.querySelector("svg.qviz");
  if (!circuitSvg) return [];
  const labelGroup = circuitSvg.querySelector("g.qubit-input-states");
  if (!labelGroup) return [];
  return Array.from(labelGroup.querySelectorAll<SVGTextElement>("text"));
};

// Non-ASCII chars are fraught with danger. Copy/paste these when possible.
// Use the following regex in VS Code to find invalid unicode chars
// [^\x20-\x7e\u{03b8}-\u{03c8}\u{2020}\u{27e8}\u{27e9}]

const mathChars = {
  theta: "θ", // \u{03b8}
  pi: "π", // \u{03c0}
  psi: "ψ", // \u{03c8}
  dagger: "†", // \u{2020}
  langle: "⟨", // \u{27e8}
  rangle: "⟩", // \u{27e9}
};

export {
  deepEqual,
  getMinGateWidth,
  getChildTargets,
  locationStringToIndexes,
  getGateLocationString,
  getMinMaxRegIdx,
  findGateElem,
  findLocation,
  findParentOperation,
  findParentArray,
  findOperation,
  getWireData,
  getToolboxElems,
  getHostElems,
  getGateElems,
  getQubitLabelElems,
  mathChars,
};
