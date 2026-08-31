// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// DOM-query helpers for the editor View layer. Everything here reaches into a rendered SVG/HTML
// tree, so these are intentionally kept out of the pure `utils.ts` module that the data and action
// layers depend on.

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

/**
 * Parse a host element's `data-wire-ys` attribute (a JSON number array of the wire-Y coordinates it
 * spans, written by the renderer) into a number array. Returns `[]` when missing or malformed.
 */
const parseWireYs = (elem: Element): number[] => {
  const wireYsAttr = elem.getAttribute("data-wire-ys");
  if (!wireYsAttr) return [];
  try {
    const parsed = JSON.parse(wireYsAttr);
    if (Array.isArray(parsed) && parsed.every((y) => typeof y === "number")) {
      return parsed;
    }
  } catch {
    // Fall through to empty array — caller decides how to handle.
  }
  return [];
};

/**
 * Find the own dashed box `<rect>` of an expanded group by its `data-location`. The box is the
 * group `<g>`'s first direct-child `<rect>` (`gate-unitary`, or `classical-container` when
 * classically controlled); restricting to direct children skips nested children's boxes. Returns
 * `null` when the group or its box isn't found (e.g. collapsed). Cloned by the shift-extend ghost.
 *
 * @param container The HTML container element containing the circuit visualization.
 * @param location  The `data-location` string of the group.
 * @returns The group's own box `<rect>`, or `null` if not found.
 */
const getGroupBoxElem = (
  container: HTMLElement,
  location: string,
): SVGGraphicsElement | null => {
  const circuitSvg = container.querySelector("svg.qviz");
  if (circuitSvg == null) return null;
  const groupElem = circuitSvg.querySelector<SVGGraphicsElement>(
    `.gate[data-location="${location}"]`,
  );
  if (groupElem == null) return null;
  for (const child of Array.from(groupElem.children)) {
    if (
      child.tagName.toLowerCase() === "rect" &&
      (child.classList.contains("gate-unitary") ||
        child.classList.contains("classical-container"))
    ) {
      return child as SVGGraphicsElement;
    }
  }
  return null;
};

export {
  findGateElem,
  getWireData,
  getToolboxElems,
  getHostElems,
  getGateElems,
  getGroupBoxElem,
  getQubitLabelElems,
  parseWireYs,
};
