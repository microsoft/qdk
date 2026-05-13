// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { GateRenderData, GateType } from "./renderer/gateRenderData.js";
import {
  minGateWidth,
  labelPaddingX,
  labelFontSize,
  argsFontSize,
  controlCircleOffset,
} from "./renderer/constants.js";
import { ComponentGrid, Operation } from "./data/circuit.js";
import { Location } from "./data/location.js";
import { Register } from "./data/register.js";

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
  classicalControlIds,
}: GateRenderData): number => {
  switch (type) {
    case GateType.Measure:
    case GateType.Cnot:
    case GateType.Swap:
      return minGateWidth;
    default: {
      // Classically controlled gates are wider because of the control button on the left
      const controlButtonWidth =
        classicalControlIds != null ? controlCircleOffset : 0;
      const labelWidth = _getStringWidth(label);
      const argsWidth =
        displayArgs != null ? _getStringWidth(displayArgs, argsFontSize) : 0;
      const textWidth = Math.max(labelWidth, argsWidth) + labelPaddingX * 2;
      return Math.max(minGateWidth, textWidth) + controlButtonWidth;
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
function getMinMaxRegIdx(operation: Operation): [number, number] {
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

  const qRegs = [...controls, ...targets].map(({ qubit }) => qubit);
  const minRegIdx: number = Math.min(...qRegs);
  const maxRegIdx: number = Math.max(...qRegs);

  return [minRegIdx, maxRegIdx];
}

/**
 * Get every `Register` referenced by an operation, including both
 * its controls and its targets/qubits/results. Returned references
 * are the live objects on the operation, so callers may mutate
 * `reg.qubit` / `reg.result` in place to renumber wires.
 *
 * Mirrors the union that `getMinMaxRegIdx` walks; centralized here
 * so the action layer and the data layer don't each re-spell the
 * per-`kind` switch.
 *
 * @param operation The operation to enumerate registers for.
 * @returns All registers (controls + targets/qubits/results) of `operation`.
 */
const getOperationRegisters = (operation: Operation): Register[] => {
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
  return [...controls, ...targets];
};

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
 * Walk a path of `[colIdx, opIdx]` segments from a root grid down through
 * nested operation children, returning the grid reached at the end.
 *
 * Returns `null` if any segment is out of bounds — for example because the
 * model has changed since the location was captured (a stale `data-location`
 * attribute on a DOM node, or a hand-constructed location that addresses an
 * op that no longer exists).
 *
 * Note: matches the long-standing semantic that an interior op missing a
 * `children` array does *not* fail the walk; the walk stays on the same
 * grid for that step. Out-of-bounds is the only thing that produces `null`.
 */
const _walkToGrid = (
  componentGrid: ComponentGrid,
  segments: ReadonlyArray<readonly [number, number]>,
): ComponentGrid | null => {
  let grid = componentGrid;
  for (const [colIdx, opIdx] of segments) {
    const col = grid[colIdx];
    if (col == null) return null;
    const op = col.components[opIdx];
    if (op == null) return null;
    grid = op.children ?? grid;
  }
  return grid;
};

/**
 * Find the parent operation of the operation specified by location.
 *
 * Navigates via [`Location`](data/location.ts) so the addressing
 * format is owned by exactly one module.
 *
 * @param componentGrid The grid of components to search through.
 * @param location The location string of the operation.
 * @returns The parent operation, or `null` if the location is empty,
 *   shallower than two segments, or addresses an op that does not exist.
 */
const findParentOperation = (
  componentGrid: ComponentGrid,
  location: string | null,
): Operation | null => {
  if (!location) return null;

  const parsed = Location.parse(location);
  // Need at least two segments: one for the op itself, one for its parent.
  if (parsed.depth < 2) return null;

  const parentOpLocation = parsed.parent();
  const parentOpSegment = parentOpLocation.last();
  if (parentOpSegment == null) return null;

  const grid = _walkToGrid(componentGrid, parentOpLocation.parent().segments);
  if (grid == null) return null;

  const [parentCol, parentOp] = parentOpSegment;
  return grid[parentCol]?.components[parentOp] ?? null;
};

/**
 * Find the parent component grid of an operation based on its location.
 *
 * Navigates via [`Location`](data/location.ts) so the addressing
 * format is owned by exactly one module.
 *
 * @param componentGrid The grid of components to search through.
 * @param location The location string of the operation.
 * @returns The parent grid of components, or `null` if the location is
 *   empty or addresses an op nested below a missing ancestor.
 */
const findParentArray = (
  componentGrid: ComponentGrid,
  location: string | null,
): ComponentGrid | null => {
  if (!location) return null;
  // Drop the last segment — it addresses the op itself; we want the grid that
  // contains it, which is keyed by its parent's segments.
  return _walkToGrid(componentGrid, Location.parse(location).parent().segments);
};

/**
 * Find an operation based on its location.
 *
 * Navigates via [`Location`](data/location.ts) so the addressing
 * format is owned by exactly one module.
 *
 * @param componentGrid The grid of components to search through.
 * @param location The location string of the operation.
 * @returns The operation at the given location, or `null` if the location
 *   is empty or addresses an op that does not exist.
 */
const findOperation = (
  componentGrid: ComponentGrid,
  location: string | null,
): Operation | null => {
  if (!location) return null;

  const last = Location.parse(location).last();
  if (last == null) return null;

  const operationParent = findParentArray(componentGrid, location);
  if (operationParent == null) return null;

  const [col, op] = last;
  return operationParent[col]?.components[op] ?? null;
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
  getGateLocationString,
  getMinMaxRegIdx,
  getOperationRegisters,
  findGateElem,
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
