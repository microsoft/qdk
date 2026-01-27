// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { ComponentGrid, Operation } from "./circuit.js";
import {
  gateHeight,
  gatePadding,
  minGateWidth,
  regLineStart,
  startX,
} from "./constants.js";
import { box, controlDot, line } from "./formatters/formatUtils.js";
import { formatGate } from "./formatters/gateFormatter.js";
import { qubitInput } from "./formatters/inputFormatter.js";
import { toRenderData } from "./panel.js";
import { Sqore } from "./sqore.js";
import {
  findLocation,
  getHostElems,
  getMinMaxRegIdx,
  getToolboxElems,
  getWireData,
  locationStringToIndexes,
} from "./utils.js";

/** Register height is the height of a single gate including the padding on the top and bottom. */
const registerHeight: number = gateHeight + gatePadding * 2;

interface Context {
  container: HTMLElement;
  svg: SVGElement;
  operationGrid: ComponentGrid;
}

/**
 * Create dropzones elements for dragging on circuit.
 *
 * @param container     HTML element for rendering visualization into
 * @param sqore         Sqore object
 */
const createDropzones = (container: HTMLElement, sqore: Sqore): void => {
  const svg = container.querySelector("svg.qviz") as SVGElement;

  const context: Context = {
    container,
    svg,
    operationGrid: sqore.circuit.componentGrid,
  };
  _addStyles(container);
  _addDataWires(container);
  svg.appendChild(_ghostQubitLayer(context));
  svg.appendChild(_dropzoneLayer(context));
};

/**
 * Creates a ghost element for dragging operations in the circuit visualization.
 *
 * @param ev The mouse event that triggered the creation of the ghost element.
 * @param container The HTML container element where the ghost element will be appended.
 * @param selectedOperation The operation that is being dragged.
 * @param isControl A boolean indicating if the ghost element is for a control operation.
 */
const createGateGhost = (
  ev: MouseEvent,
  container: HTMLElement,
  selectedOperation: Operation,
  isControl: boolean,
) => {
  const ghost = isControl
    ? controlDot(20, 20, [])
    : (() => {
        const ghostRenderData = toRenderData(selectedOperation, 0, 0);
        return formatGate(ghostRenderData).cloneNode(true) as SVGElement;
      })();

  _createGhostElement(container, ev, ghost, isControl);
};

/**
 * Creates a ghost element for dragging a qubit line label.
 *
 * @param ev The mouse event that triggered the drag.
 * @param container The HTML container element where the ghost will be appended.
 * @param labelElem The SVGTextElement representing the qubit label to be cloned (including any tspans or formatting).
 */
const createQubitLabelGhost = (
  ev: MouseEvent,
  container: HTMLElement,
  labelElem: SVGTextElement,
) => {
  const ghostGate: Operation = {
    kind: "unitary",
    gate: "?", // This will be replaced by the label elem
    targets: [],
  };
  const ghostRenderData = toRenderData(ghostGate, 0, 0);
  const ghost = formatGate(ghostRenderData) as SVGElement;

  // Replace the placeholder text with the label element
  const placeholderText = ghost.querySelector(".qs-maintext");
  if (placeholderText) {
    // Remove all children from placeholderText
    while (placeholderText.firstChild) {
      placeholderText.removeChild(placeholderText.firstChild);
    }
    // Clone and append each child from labelElem
    for (const child of Array.from(labelElem.childNodes)) {
      placeholderText.appendChild(child.cloneNode(true));
    }
    placeholderText.setAttribute(
      "font-size",
      labelElem.getAttribute("font-size") || "16",
    );
  }

  _createGhostElement(container, ev, ghost, false);
};

/**
 * Creates and appends a draggable "ghost" element to the DOM for visual feedback during drag operations.
 *
 * @param container The HTML container element to which the ghost element will be appended.
 * @param ev The MouseEvent that triggered the drag, used to position the ghost.
 * @param ghost The SVGElement representing the visual ghost to be dragged.
 * @param isControl Boolean indicating if the ghost is for a control operation (affects sizing).
 */
const _createGhostElement = (
  container: HTMLElement,
  ev: MouseEvent,
  ghost: SVGElement,
  isControl: boolean,
) => {
  // Generate svg element to wrap around ghost element
  const svgElem = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svgElem.append(ghost);

  // Generate div element to wrap around svg element
  const divElem = document.createElement("div");
  divElem.classList.add("ghost");
  divElem.appendChild(svgElem);
  divElem.style.position = "fixed";

  if (container) {
    container.appendChild(divElem);

    // Now that the element is appended to the DOM, get its dimensions
    const [ghostWidth, ghostHeight] = isControl
      ? [40, 40]
      : (() => {
          const ghostRect = ghost.getBoundingClientRect();
          return [ghostRect.width, ghostRect.height];
        })();

    const updateDivLeftTop = (ev: MouseEvent) => {
      divElem.style.left = `${ev.clientX - ghostWidth / 2}px`;
      divElem.style.top = `${ev.clientY - ghostHeight / 2}px`;
    };

    updateDivLeftTop(ev);

    const cleanup = () => {
      container.removeEventListener("mousemove", updateDivLeftTop);
      document.removeEventListener("mouseup", cleanup);
      if (divElem.parentNode) {
        divElem.parentNode.removeChild(divElem);
      }
    };

    container.addEventListener("mousemove", updateDivLeftTop);
    document.addEventListener("mouseup", cleanup);
  } else {
    console.error("container not found");
  }
};

/**
 * Create a dropzone element that spans the length of the wire.
 *
 * @param circuitSvg The SVG element representing the circuit.
 * @param wireData An array of y values corresponding to the circuit wires.
 * @param wireIndex The index of the wire or the "between" position.
 * @param isBetween If true, creates a dropzone between wires.
 * @returns The created dropzone SVG element.
 */
const createWireDropzone = (
  circuitSvg: SVGElement,
  wireData: number[],
  wireIndex: number,
  isBetween: boolean = false,
): SVGElement => {
  const svgWidth = Number(circuitSvg.getAttribute("width"));
  const paddingY = 20;
  let wireY: number;

  if (isBetween) {
    // Dropzone BETWEEN wires (including before first and after last)
    if (wireIndex === wireData.length) {
      wireY = wireData[wireData.length - 1] + registerHeight / 2;
    } else {
      wireY = wireData[wireIndex] - registerHeight / 2;
    }
  } else {
    // Dropzone ON the wire
    wireY = wireData[wireIndex];
  }

  const dropzone = box(
    0,
    wireY - paddingY,
    svgWidth,
    paddingY * 2,
    "dropzone-full-wire",
  );
  dropzone.setAttribute("data-dropzone-wire", `${wireIndex}`);

  return dropzone;
};

/**
 * Remove all wire dropzones.
 *
 * @param circuitSvg The SVG element representing the circuit.
 */
const removeAllWireDropzones = (circuitSvg: SVGElement) => {
  const dropzones = circuitSvg.querySelectorAll(".dropzone-full-wire");
  dropzones.forEach((elem) => {
    elem.parentNode?.removeChild(elem);
  });
};

/**
 * Add data-wire to all host elements
 */
const _addDataWires = (container: HTMLElement) => {
  const elems = getHostElems(container);
  elems.forEach((elem) => {
    const wireYs = _wireYs(elem);
    // i.e. wireYs = [40], wireData returns [40, 100, 140, 180]
    // dataWire will return 0, which is the index of 40 in wireData
    const dataWire = getWireData(container).findIndex((y) =>
      wireYs.includes(y),
    );
    if (dataWire !== -1) {
      elem.setAttribute("data-wire", `${dataWire}`);
    }
  });
};

/**
 * Create a list of wires that element is spanning on
 * i.e. Gate 'Foo' spans on wire 0 (y=40), 1 (y=100), and 2 (y=140)
 *      Function returns [40, 100, 140]
 */
const _wireYs = (elem: SVGGraphicsElement): number[] => {
  const wireYsAttr = elem.getAttribute("data-wire-ys");
  if (wireYsAttr) {
    try {
      const wireYs = JSON.parse(wireYsAttr);
      if (Array.isArray(wireYs) && wireYs.every((y) => typeof y === "number")) {
        return wireYs;
      }
    } catch {
      console.warn(`Invalid data-wire-ys attribute: ${wireYsAttr}`);
    }
  }
  return [];
};

/**
 * Add custom styles specific to this module
 */
const _addStyles = (container: HTMLElement): void => {
  const elems = getHostElems(container);
  elems.forEach((elem) => {
    if (_wireYs(elem).length < 2) elem.style.cursor = "grab";
  });

  const toolBoxElems = getToolboxElems(container);
  toolBoxElems.forEach((elem) => {
    elem.style.cursor = "grab";
  });
};

/**
 * Create layer with ghost qubit wire and label
 */
const _ghostQubitLayer = (context: Context) => {
  const { container, svg } = context;

  const wireData = getWireData(container);

  const svgHeight = Number(svg.getAttribute("height") || svg.clientHeight || 0);
  const svgWidth = Number(svg.getAttribute("width") || svg.clientWidth || 800);
  const ghostY = svgHeight;

  const ghostLayer = document.createElementNS(
    "http://www.w3.org/2000/svg",
    "g",
  );
  ghostLayer.classList.add("ghost-qubit-layer");
  ghostLayer.style.display = "none";
  // Insert before dropzone-layer if possible, otherwise at end
  const dzLayer = svg.querySelector("g.dropzone-layer");
  if (dzLayer) {
    svg.insertBefore(ghostLayer, dzLayer);
  } else {
    svg.appendChild(ghostLayer);
  }

  const ghostWire = line(
    regLineStart,
    ghostY,
    svgWidth,
    ghostY,
    "qubit-wire ghost-opacity",
  );

  const ghostLabel = qubitInput(
    ghostY,
    wireData.length,
    wireData.length.toString(),
  );
  ghostLabel.classList.add("ghost-opacity");
  ghostLayer.appendChild(ghostWire);
  ghostLayer.appendChild(ghostLabel);

  context.svg.setAttribute("height", (svgHeight + registerHeight).toString());
  svg.setAttribute("viewBox", `0 0 ${svgWidth} ${svgHeight + registerHeight}`);

  return ghostLayer;
};

/**
 * Create dropzone layer with all dropzones populated
 */
const _dropzoneLayer = (context: Context) => {
  const dropzoneLayer = document.createElementNS(
    "http://www.w3.org/2000/svg",
    "g",
  );
  dropzoneLayer.classList.add("dropzone-layer");
  dropzoneLayer.style.display = "none";

  const { container, operationGrid } = context;

  const colArray = getColumnOffsetsAndWidths(container);
  const wireData = getWireData(container);

  // Create dropzones for each intersection of columns and wires
  for (let colIndex = 0; colIndex < colArray.length; colIndex++) {
    const columnOps = operationGrid[colIndex];
    let wireIndex = 0;

    const makeBox = (opIndex: number, interColumn: boolean) =>
      makeDropzoneBox(
        colIndex,
        opIndex,
        colArray,
        wireData,
        wireIndex,
        interColumn,
      );

    columnOps.components.forEach((op, opIndex) => {
      const [minTarget, maxTarget] = getMinMaxRegIdx(op, wireData.length);
      // Add dropzones before the first target
      while (wireIndex <= maxTarget) {
        dropzoneLayer.appendChild(makeBox(opIndex, true));
        // We don't want to make a central zone if the spot is occupied by a gate or its connecting lines
        if (wireIndex < minTarget) {
          dropzoneLayer.appendChild(makeBox(opIndex, false));
        }

        wireIndex++;
      }
    });

    // Add dropzones after the last target
    while (wireIndex < wireData.length) {
      dropzoneLayer.appendChild(makeBox(columnOps.components.length, true));
      dropzoneLayer.appendChild(makeBox(columnOps.components.length, false));

      wireIndex++;
    }
  }

  // This assumes column indexes are continuous
  const endColIndex = colArray.length;

  // Add remaining dropzones to allow users to add gates to the end of the circuit
  for (let wireIndex = 0; wireIndex < wireData.length; wireIndex++) {
    const dropzone = makeDropzoneBox(
      endColIndex,
      0,
      colArray,
      wireData,
      wireIndex,
      true,
    );
    // Note: the last column should have the shape of an inter-column dropzone, but
    // we don't want to attach the inter-column logic to it.
    dropzone.setAttribute("data-dropzone-inter-column", "false");
    dropzoneLayer.appendChild(dropzone);
  }

  return dropzoneLayer;
};

/**
 * Computes a sorted array of { xOffset, colWidth } for each column index.
 * The array index corresponds to the column index.
 *
 * @param container The circuit container element.
 * @returns Array where arr[colIndex] = { xOffset, colWidth }
 */
const getColumnOffsetsAndWidths = (
  container: HTMLElement,
): { xOffset: number; colWidth: number }[] => {
  const elems = getHostElems(container);

  if (elems.length === 0) {
    return [];
  }

  // Compute column widths
  const colWidths = elems.reduce(
    (acc, elem) => {
      const widthAttr = elem.getAttribute("data-width");
      if (!widthAttr) return acc;
      const elemWidth: number = Number(widthAttr);
      if (isNaN(elemWidth) || elemWidth <= 0) return acc;

      const location = findLocation(elem);
      if (!location) return acc;
      const indexes = locationStringToIndexes(location);
      if (indexes.length != 1) return acc;
      const [colIndex] = indexes[0];
      if (!acc[colIndex]) {
        acc[colIndex] = Math.max(minGateWidth, elemWidth);
      } else {
        acc[colIndex] = Math.max(acc[colIndex], elemWidth);
      }
      return acc;
    },
    {} as Record<number, number>,
  );

  // Find the max colIndex to size the array
  const maxColIndex = Math.max(...Object.keys(colWidths).map(Number), 0);

  let xOffset = startX - gatePadding;
  const result: { xOffset: number; colWidth: number }[] = [];
  for (let colIndex = 0; colIndex <= maxColIndex; colIndex++) {
    const colWidth = colWidths[colIndex] ?? minGateWidth;
    result[colIndex] = { xOffset, colWidth };
    xOffset += colWidth + gatePadding * 2;
  }
  return result;
};

/**
 * Create a dropzone box element.
 *
 * @param colIndex The index of the column where the dropzone is located.
 * @param opIndex The index of the operation within the column.
 * @param colArray   An array of objects containing xOffset and colWidth for each column.
 * @param wireData The array of wire Y positions.
 * @param wireIndex The index of the wire for which the dropzone is created.
 * @param interColumn Whether the dropzone is between columns.
 *
 * @returns The created dropzone SVG element.
 */
const makeDropzoneBox = (
  colIndex: number,
  opIndex: number,
  colArray: { xOffset: number; colWidth: number }[],
  wireData: number[],
  wireIndex: number,
  interColumn: boolean,
): SVGElement => {
  const wireY = wireData[wireIndex];
  let xOffset: number, colWidth: number;

  if (colArray[colIndex]) {
    ({ xOffset, colWidth } = colArray[colIndex]);
  } else {
    // Compute offset for a hypothetical new last column
    const last = colArray[colArray.length - 1];
    if (last) {
      xOffset = last.xOffset + last.colWidth + gatePadding * 2;
    } else {
      // If there are no columns at all, start at initial offset
      xOffset = startX - gatePadding;
    }
    colWidth = minGateWidth;
  }

  const paddingY = 20;
  let dropzone = null;
  if (interColumn) {
    dropzone = box(
      xOffset - gatePadding * 2,
      wireY - paddingY,
      gatePadding * 4,
      paddingY * 2,
      "dropzone",
    );
  } else {
    dropzone = box(
      xOffset + gatePadding,
      wireY - paddingY,
      colWidth,
      paddingY * 2,
      "dropzone",
    );
  }
  dropzone.setAttribute("data-dropzone-location", `${colIndex},${opIndex}`);
  dropzone.setAttribute("data-dropzone-wire", `${wireIndex}`);
  dropzone.setAttribute("data-dropzone-inter-column", `${interColumn}`);
  return dropzone;
};

export {
  createDropzones,
  createGateGhost,
  createQubitLabelGhost,
  createWireDropzone,
  removeAllWireDropzones,
  getColumnOffsetsAndWidths,
  makeDropzoneBox,
};
