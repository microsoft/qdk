// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { ComponentGrid, Operation } from "../data/circuit.js";
import {
  gateHeight,
  gatePadding,
  minGateWidth,
  regLineStart,
  startX,
} from "../renderer/constants.js";
import { box, controlDot, line } from "../renderer/formatters/formatUtils.js";
import { formatGate } from "../renderer/formatters/gateFormatter.js";
import { qubitInput } from "../renderer/formatters/inputFormatter.js";
import { LayoutMap, LayoutScope } from "../renderer/layoutMap.js";
import { Location } from "../data/location.js";
import { toRenderData } from "./panel.js";
import { isExpandedGroup, Sqore } from "../sqore.js";
import {
  getHostElems,
  getMinMaxRegIdx,
  getToolboxElems,
  getWireData,
} from "../utils.js";

/** Register height is the height of a single gate including the padding on the top and bottom. */
const registerHeight: number = gateHeight + gatePadding * 2;

interface Context {
  container: HTMLElement;
  svg: SVGElement;
  operationGrid: ComponentGrid;
  /**
   * Geometry from the layout pass. Source of truth for dropzone
   * positioning. See [`layoutMap.ts`](layoutMap.ts).
   */
  layoutMap: LayoutMap;
}

/**
 * Create dropzones elements for dragging on circuit.
 *
 * R6 wraps every editor-only DOM node inside a single
 * `<g class="editor-overlay">` group, attached as the last child of
 * `svg.qviz`. All editor-only content (dropzones, the ghost qubit
 * wire, plus any future selection rectangles / hover halos /
 * Inspector anchors) lives inside that group; the renderer-owned
 * children of `svg.qviz` (gates, wires, register labels) stay
 * purely presentational.
 *
 * @param container     HTML element for rendering visualization into
 * @param sqore         Sqore object
 * @param layoutMap     Geometry captured during the layout pass
 * @returns The editor overlay `<g>` so callers can attach further
 *          editor-only DOM (e.g. wire dropzones spawned during a
 *          drag) without having to re-query the SVG.
 */
const createDropzones = (
  container: HTMLElement,
  sqore: Sqore,
  layoutMap: LayoutMap,
): SVGGElement => {
  const svg = container.querySelector("svg.qviz") as SVGElement;

  const overlay = document.createElementNS(
    "http://www.w3.org/2000/svg",
    "g",
  ) as SVGGElement;
  overlay.classList.add("editor-overlay");
  // Append last so the overlay paints over the rendered content.
  svg.appendChild(overlay);

  const context: Context = {
    container,
    svg,
    operationGrid: sqore.circuit.componentGrid,
    layoutMap,
  };
  _addStyles(container);
  _addDataWires(container);

  // Layer z-order inside the overlay (later children paint on top):
  //   1. ghost-qubit-layer — the trailing add-a-qubit row.
  //   2. dropzone-layer — catches mouseup for gate drops.
  // Wire dropzones spawned during a drag get appended to `overlay`
  // directly (above both), keeping them above the static layers but
  // still within the editor's territory.
  overlay.appendChild(_ghostQubitLayer(context));
  overlay.appendChild(_dropzoneLayer(context));

  return overlay;
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
 * Create the ghost-qubit layer — the trailing add-a-qubit row that
 * appears below the last real wire when a drag is in progress.
 *
 * Pure-create after R6: returns the layer ready to be attached by
 * the caller (`createDropzones` puts it inside the editor overlay).
 * The one true side effect kept here is extending the SVG's
 * `height` / `viewBox` to make room for the trailing ghost wire —
 * that's a renderer-side dimension change, not editor DOM, so it
 * lives at the SVG root.
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

  // Extend the rendered SVG so the trailing ghost row is visible.
  // (Touches the SVG root, not the overlay — the height change is a
  // renderer-side dimension, independent of the editor DOM.)
  context.svg.setAttribute("height", (svgHeight + registerHeight).toString());
  svg.setAttribute("viewBox", `0 0 ${svgWidth} ${svgHeight + registerHeight}`);

  return ghostLayer;
};

/**
 * Create dropzone layer with all dropzones populated.
 *
 * Walks the component grid recursively: top-level columns get
 * dropzones, and any expanded group's body gets its own nested
 * dropzones with hierarchical location strings (e.g. `0,0-1,2` for
 * the op at column 1 / opIndex 2 inside the expanded group at
 * top-level column 0 / opIndex 0). Those nested location strings are
 * exactly what `findParentArray` / `addOperation` / `moveOperation`
 * already understand, so no plumbing further down the call chain
 * needs to change.
 *
 * Coordinates come from `context.layoutMap` — the same numbers the
 * layout pass computed when rendering the gates. There is no
 * geometry recovery from SVG attributes; that approach was the
 * source of the Phase A bug.
 */
const _dropzoneLayer = (context: Context) => {
  const dropzoneLayer = document.createElementNS(
    "http://www.w3.org/2000/svg",
    "g",
  );
  dropzoneLayer.classList.add("dropzone-layer");
  dropzoneLayer.style.display = "none";

  const { container, operationGrid, layoutMap } = context;
  const wireData = getWireData(container);

  // Recurse from the top-level scope. Wire extent at top level covers
  // every wire in the circuit; nested scopes pass a tightened extent
  // matching their parent group's [minTarget, maxTarget].
  _populateDropzonesForGrid(
    dropzoneLayer,
    layoutMap,
    operationGrid,
    "",
    wireData,
    0,
    wireData.length,
  );

  // Trailing-append column — only at top level. Lets the user add a
  // gate to a brand-new column past the rightmost existing one. No
  // analogue inside a nested group; nested grids grow by inserting
  // into existing columns.
  _appendTrailingColumn(dropzoneLayer, layoutMap, wireData);

  return dropzoneLayer;
};

/**
 * Append the trailing-column dropzones (one per wire) past the
 * rightmost top-level column. Each is shaped like an inter-column
 * band but tagged `data-dropzone-inter-column="false"` so the
 * mouseup handler treats it as a normal drop target rather than an
 * insert-between-columns operation.
 */
const _appendTrailingColumn = (
  dropzoneLayer: SVGElement,
  layoutMap: LayoutMap,
  wireData: number[],
): void => {
  const topScope = layoutMap.scopes.get("");
  if (topScope == null) return;
  const trailingColIndex = topScope.columnXOffsets.length;
  for (let wireIndex = 0; wireIndex < wireData.length; wireIndex++) {
    const dropzone = makeDropzoneBox(
      trailingColIndex,
      0,
      topScope,
      wireData,
      wireIndex,
      true,
    );
    dropzone.setAttribute("data-dropzone-inter-column", "false");
    dropzoneLayer.appendChild(dropzone);
  }
};

/**
 * Emit dropzones for one scope (top-level grid or one expanded group's
 * children grid) into `dropzoneLayer`, then recurse for each expanded
 * group inside this scope.
 *
 * Coordinates are sourced from `layoutMap.scopes.get(pathPrefix)`,
 * which holds the *exact* per-column x-offsets and widths the layout
 * pass computed for this scope.
 *
 * @param dropzoneLayer  Mutable accumulator — every dropzone produced
 *                       at any depth is appended here.
 * @param layoutMap      Geometry from the layout pass. Looked up by
 *                       `pathPrefix` to find this scope's column
 *                       offsets/widths.
 * @param grid           The grid of components for this scope.
 * @param pathPrefix     Hierarchical location prefix for this scope.
 *                       `""` at top level; `"0,0"` for the children of
 *                       the top-level op at column 0 / opIndex 0;
 *                       `"0,0-1,2"` for grandchildren; etc. Doubles as
 *                       the `LayoutMap.scopes` key.
 * @param wireData       Full circuit wire-Y array (wires don't get
 *                       reindexed inside groups; child operations still
 *                       reference circuit-wide qubit IDs).
 * @param minWire        Inclusive lower bound on wire indices this
 *                       scope is allowed to produce dropzones for.
 *                       At top level this is `0`; for nested scopes
 *                       it's the parent group's top wire so a drop
 *                       inside `Foo` (which spans wires 0-1) can never
 *                       land on wire 2.
 * @param maxWire        Exclusive upper bound, mirror of `minWire`.
 */
const _populateDropzonesForGrid = (
  dropzoneLayer: SVGElement,
  layoutMap: LayoutMap,
  grid: ComponentGrid,
  pathPrefix: string,
  wireData: number[],
  minWire: number,
  maxWire: number,
): void => {
  const scope = layoutMap.scopes.get(pathPrefix);
  // Defensive fallback: if the scope wasn't recorded (shouldn't happen
  // for any expanded scope, since `processOperations` records each one
  // it processes), skip rather than emit garbage dropzones.
  if (scope == null) return;

  // The LayoutMap's column count matches the rendered grid (one entry
  // per processed column); using `Math.max` is just defensive against
  // unexpected mismatch.
  const colCount = Math.max(scope.columnXOffsets.length, grid.length);

  for (let colIndex = 0; colIndex < colCount; colIndex++) {
    const columnOps = grid[colIndex];
    if (columnOps == null) continue;

    // Track the next wire we still need to cover in *this scope*.
    // Starts at the scope's top wire so we don't accidentally emit
    // dropzones above an expanded group's interior.
    let wireIndex = minWire;

    const makeBox = (opIndex: number, interColumn: boolean) =>
      makeDropzoneBox(
        colIndex,
        opIndex,
        scope,
        wireData,
        wireIndex,
        interColumn,
        pathPrefix,
      );

    columnOps.components.forEach((op, opIndex) => {
      const [minTarget, maxTarget] = getMinMaxRegIdx(op);
      // Defensive clip: an op inside a group should never reference a
      // wire outside the group's extent, but if the data ever drifts
      // we'd rather skip than crash or emit junk dropzones.
      const opMaxTarget = Math.min(maxTarget, maxWire - 1);
      while (wireIndex <= opMaxTarget) {
        dropzoneLayer.appendChild(makeBox(opIndex, true));
        // Don't make a central zone if the spot is occupied by a gate
        // or its connecting lines — only above the op's first target.
        if (wireIndex < minTarget) {
          dropzoneLayer.appendChild(makeBox(opIndex, false));
        }
        wireIndex++;
      }

      // If this op is itself an expanded group, recurse so its
      // interior also gets dropzones. The recursion's wire extent
      // matches the group's own [minTarget, maxTarget] (inclusive),
      // ensuring nested dropzones can never escape the parent group.
      if (isExpandedGroup(op) && op.children != null) {
        _populateDropzonesForGrid(
          dropzoneLayer,
          layoutMap,
          op.children,
          composeLocation(pathPrefix, colIndex, opIndex),
          wireData,
          minTarget,
          maxTarget + 1,
        );
      }
    });

    // Cover wires below the last op in this column, still within scope.
    while (wireIndex < maxWire) {
      dropzoneLayer.appendChild(makeBox(columnOps.components.length, true));
      dropzoneLayer.appendChild(makeBox(columnOps.components.length, false));
      wireIndex++;
    }
  }
};

/**
 * Half-width of an inter-column dropzone band, in svg units. The band
 * straddles the gap between two columns; total band width is
 * `INTER_COLUMN_HALF_WIDTH * 2`.
 */
const INTER_COLUMN_HALF_WIDTH = gatePadding * 2;

/** Vertical padding above/below each dropzone, in svg units. */
const DROPZONE_PADDING_Y = 20;

/**
 * Geometry for one column inside one scope. Either looks up an
 * existing column from `LayoutScope`, or synthesizes a "trailing"
 * position past the rightmost column for the append-new-column
 * dropzone.
 *
 * Returned `colStartX` is the column's left edge in absolute svg
 * coords — the same value as `_fillRenderDataX`'s `colStartX[i]`.
 */
const columnGeometry = (
  scope: LayoutScope,
  colIndex: number,
): { colStartX: number; colWidth: number } => {
  if (colIndex < scope.columnXOffsets.length) {
    return {
      colStartX: scope.columnXOffsets[colIndex],
      colWidth: scope.columnWidths[colIndex] ?? minGateWidth,
    };
  }
  // Synthesize a column past the rightmost. Spacing matches the
  // historical accumulator (`gatePadding * 2` between columns).
  const lastIndex = scope.columnXOffsets.length - 1;
  if (lastIndex >= 0) {
    const lastStart = scope.columnXOffsets[lastIndex];
    const lastWidth = scope.columnWidths[lastIndex] ?? minGateWidth;
    return {
      colStartX: lastStart + lastWidth + gatePadding * 2,
      colWidth: minGateWidth,
    };
  }
  return { colStartX: startX, colWidth: minGateWidth };
};

/**
 * Compose a hierarchical dropzone location string for `(prefix, col, op)`.
 *
 *   composeLocation("",     0, 1) -> "0,1"
 *   composeLocation("0,0",  1, 2) -> "0,0-1,2"
 *
 * Thin wrapper over `Location` for the wire-format dropzone attrs.
 */
const composeLocation = (
  prefix: string,
  colIndex: number,
  opIndex: number,
): string => Location.parse(prefix).child(colIndex, opIndex).toString();

/**
 * Create a dropzone box element.
 *
 * @param colIndex     Column the dropzone belongs to. May equal
 *                     `scope.columnXOffsets.length` to address the
 *                     trailing-append column past the rightmost.
 * @param opIndex      Operation index inside the column.
 * @param scope        Layout geometry for the *scope* this dropzone
 *                     belongs to. Source of truth — comes straight
 *                     from `LayoutMap.scopes.get(pathPrefix)`.
 * @param wireData     Wire Y positions in absolute svg coords.
 * @param wireIndex    Index into `wireData` for this dropzone's row.
 * @param interColumn  `true` for the narrow band straddling the
 *                     left edge of `colIndex`; `false` for the
 *                     full-width box covering the column.
 * @param pathPrefix   Hierarchical scope prefix; `""` at top level.
 *                     Composed into `data-dropzone-location` so
 *                     `findParentArray` walks into the right
 *                     `children` grid on drop.
 */
const makeDropzoneBox = (
  colIndex: number,
  opIndex: number,
  scope: LayoutScope,
  wireData: number[],
  wireIndex: number,
  interColumn: boolean,
  pathPrefix: string = "",
): SVGElement => {
  const wireY = wireData[wireIndex];
  const { colStartX, colWidth } = columnGeometry(scope, colIndex);

  const dropzone = interColumn
    ? // Inter-column band: centered on the left edge of `colIndex`,
      // i.e. on the gap between this column and the previous one.
      box(
        colStartX - INTER_COLUMN_HALF_WIDTH - gatePadding,
        wireY - DROPZONE_PADDING_Y,
        INTER_COLUMN_HALF_WIDTH * 2,
        DROPZONE_PADDING_Y * 2,
        "dropzone",
      )
    : // On-column box: covers exactly `[colStartX, colStartX + colWidth]`,
      // which is the gate's bounding box width-wise.
      box(
        colStartX,
        wireY - DROPZONE_PADDING_Y,
        colWidth,
        DROPZONE_PADDING_Y * 2,
        "dropzone",
      );

  dropzone.setAttribute(
    "data-dropzone-location",
    composeLocation(pathPrefix, colIndex, opIndex),
  );
  dropzone.setAttribute("data-dropzone-wire", `${wireIndex}`);
  dropzone.setAttribute("data-dropzone-inter-column", `${interColumn}`);
  return dropzone;
};

export {
  createDropzones,
  createGateGhost,
  createQubitLabelGhost,
  createWireDropzone,
  makeDropzoneBox,
  removeAllWireDropzones,
};
