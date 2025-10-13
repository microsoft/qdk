// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { labelFontSize, svgNS } from "../constants.js";

// Helper functions for basic SVG components

/**
 * Create an SVG element.
 *
 * @param type The type of element to be created.
 * @param attributes The attributes that define the element.
 *
 * @returns SVG element.
 */
export const createSvgElement = (
  type: string,
  attributes: { [attr: string]: string } = {},
): SVGElement => {
  const el: SVGElement = document.createElementNS(svgNS, type);
  Object.entries(attributes).forEach(([attrName, attrVal]) =>
    el.setAttribute(attrName, attrVal),
  );
  return el;
};

/**
 * Given an array of SVG elements, group them as an SVG group using the `<g>` tag.
 *
 * @param svgElems   Array of SVG elements.
 * @param attributes Key-value pairs of attributes and they values.
 *
 * @returns SVG element for grouped elements.
 */
export const group = (
  svgElems: SVGElement[],
  attributes: { [attr: string]: string } = {},
): SVGElement => {
  const el: SVGElement = createSvgElement("g", attributes);
  svgElems.forEach((child: SVGElement) => el.appendChild(child));
  return el;
};

/**
 * Generate an SVG line.
 *
 * @param x1        x coord of starting point of line.
 * @param y1        y coord of starting point of line.
 * @param x2        x coord of ending point of line.
 * @param y2        y coord fo ending point of line.
 * @param className Class name of element.
 *
 * @returns SVG element for line.
 */
export const line = (
  x1: number,
  y1: number,
  x2: number,
  y2: number,
  className?: string,
): SVGElement => {
  const attrs: { [attr: string]: string } = {
    x1: x1.toString(),
    x2: x2.toString(),
    y1: y1.toString(),
    y2: y2.toString(),
  };
  if (className != null) attrs["class"] = className;
  return createSvgElement("line", attrs);
};

/**
 * Generate an SVG circle.
 *
 * @param x      x coord of circle.
 * @param y      y coord of circle.
 * @param radius Radius of circle.
 *
 * @returns SVG element for circle.
 */
export const circle = (
  x: number,
  y: number,
  radius: number,
  className?: string,
): SVGElement => {
  const attrs: { [attr: string]: string } = {
    cx: x.toString(),
    cy: y.toString(),
    r: radius.toString(),
  };
  if (className != null) attrs["class"] = className;
  return createSvgElement("circle", attrs);
};

/**
 * Generate the SVG representation of a control dot used for controlled operations.
 *
 * @param x      x coord of circle.
 * @param y      y coord of circle.
 * @param radius Radius of circle.
 *
 * @returns SVG element for control dot.
 */
export const controlDot = (x: number, y: number, radius = 5): SVGElement =>
  circle(x, y, radius, "control-dot");

/**
 * Generate the SVG representation of a unitary box that represents an arbitrary unitary operation.
 *
 * @param x         x coord of box.
 * @param y         y coord of box.
 * @param width     Width of box.
 * @param height    Height of box.
 * @param className Class name of element.
 *
 * @returns SVG element for unitary box.
 */
export const box = (
  x: number,
  y: number,
  width: number,
  height: number,
  className = "gate-unitary",
): SVGElement =>
  createSvgElement("rect", {
    class: className,
    x: x.toString(),
    y: y.toString(),
    width: width.toString(),
    height: height.toString(),
  });

/**
 * Generate the SVG text element from a given text string.
 *
 * @param text String to render as SVG text.
 * @param x    Middle x coord of text.
 * @param y    Middle y coord of text.
 * @param fs   Font size of text.
 *
 * @returns SVG element for text.
 */
export const text = (
  text: string,
  x: number,
  y: number,
  fs: number = labelFontSize,
): SVGTextElement => {
  const el = createSvgElement("text", {
    "font-size": fs.toString(),
    x: x.toString(),
    y: y.toString(),
  }) as SVGTextElement;
  el.textContent = text;
  return el;
};

/**
 * Generate the SVG representation of the arc used in the measurement box.
 *
 * @param x  x coord of arc.
 * @param y  y coord of arc.
 * @param rx x radius of arc.
 * @param ry y radius of arc.
 *
 * @returns SVG element for arc.
 */
export const arc = (x: number, y: number, rx: number, ry: number): SVGElement =>
  createSvgElement("path", {
    class: "arc-measure",
    d: `M ${x + 2 * rx} ${y} A ${rx} ${ry} 0 0 0 ${x} ${y}`,
  });

/**
 * Generate a dashed SVG line.
 *
 * @param x1        x coord of starting point of line.
 * @param y1        y coord of starting point of line.
 * @param x2        x coord of ending point of line.
 * @param y2        y coord fo ending point of line.
 * @param className Class name of element.
 *
 * @returns SVG element for dashed line.
 */
export const dashedLine = (
  x1: number,
  y1: number,
  x2: number,
  y2: number,
  className?: string,
): SVGElement => {
  const el: SVGElement = line(x1, y1, x2, y2, className);
  el.setAttribute("stroke-dasharray", "8, 8");
  return el;
};

/**
 * Generate the SVG representation of the dashed box used for enclosing groups of operations controlled on a classical register.
 *
 * @param x         x coord of box.
 * @param y         y coord of box.
 * @param width     Width of box.
 * @param height    Height of box.
 * @param className Class name of element.
 *
 * @returns SVG element for dashed box.
 */
export const dashedBox = (
  x: number,
  y: number,
  width: number,
  height: number,
  className?: string,
): SVGElement => {
  const el: SVGElement = box(x, y, width, height, className);
  el.setAttribute("fill-opacity", "0");
  el.setAttribute("stroke-dasharray", "8, 8");
  return el;
};
