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
import { toRenderData } from "./standaloneRenderData.js";
import { Sqore } from "../sqore.js";
import {
  getHostElems,
  getQuantumWireRange,
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
   * positioning. See [`layoutMap.ts`](../renderer/layoutMap.ts).
   */
  layoutMap: LayoutMap;
}

/**
 * Create dropzone elements for dragging on the circuit.
 *
 * Every editor-only DOM node lives inside a single
 * `<g class="editor-overlay">` group attached as the last child of
 * `svg.qviz`, so the renderer-owned children (gates, wires, labels)
 * stay purely presentational.
 *
 * @param container     HTML element for rendering visualization into
 * @param sqore         Sqore object
 * @param layoutMap     Geometry captured during the layout pass
 * @returns The editor overlay `<g>` so callers can attach further
 *          editor-only DOM (e.g. wire dropzones spawned during a
 *          drag).
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
  // Wire dropzones spawned during a drag append to `overlay`
  // directly, keeping them above both static layers.
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
 * appears below the last real wire while a drag is in progress.
 *
 * Returns the layer ready to be attached by the caller. The one side
 * effect kept here is extending the SVG's `height` / `viewBox` to
 * make room for the trailing ghost wire — a renderer-side dimension,
 * so it lives at the SVG root rather than on the overlay.
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
  // (Touches the SVG root, not the overlay — a renderer-side
  // dimension.)
  context.svg.setAttribute("height", (svgHeight + registerHeight).toString());
  svg.setAttribute("viewBox", `0 0 ${svgWidth} ${svgHeight + registerHeight}`);

  return ghostLayer;
};

/**
 * Create dropzone layer with all dropzones populated.
 *
 * Walks the component grid recursively: top-level columns get
 * dropzones, and any expanded group's body gets nested dropzones
 * with hierarchical location strings (e.g. `0,0-1,2`). Those strings
 * are exactly what `findParentArray` / `addOperation` /
 * `moveOperation` already understand.
 *
 * Coordinates come from `context.layoutMap` — the same numbers the
 * layout pass computed when rendering the gates. Geometry is never
 * recovered from SVG attributes.
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
  // every wire; nested scopes pass a tightened extent matching their
  // group's [minTarget, maxTarget]. Each scope's trailing-append
  // column is emitted by `_populateDropzonesForGrid` itself.
  _populateDropzonesForGrid(
    dropzoneLayer,
    layoutMap,
    operationGrid,
    "",
    wireData,
    0,
    wireData.length,
  );

  return dropzoneLayer;
};

/**
 * Append a trailing-column band of dropzones (one per wire in
 * `[minWire, maxWire)`) just past the rightmost column of a single
 * scope — either the top-level grid or an expanded group's children
 * grid.
 *
 * Each emitted dropzone is shaped like the existing left-edge
 * inter-column band (so it visually reads as "I'm extending this
 * scope to the right"), but tagged
 * `data-dropzone-inter-column="false"` so the drop handler treats it
 * as a normal drop. The `_addOp` action takes care of synthesizing the
 * new column when the target column index is one past the rightmost.
 *
 * Together with the
 * leading-column band that already falls out of the
 * `_populateDropzonesForGrid` loop at `colIndex=0`, this gives every
 * expanded group a one-column-of-reach extend-sideways gesture on both
 * edges, no modifier required.
 *
 * Idempotent w.r.t. wire extent: at the top level, `[minWire, maxWire)`
 * is `[0, wireData.length)`. For nested scopes it's the parent group's
 * own wire span, so the trailing column can't escape the group's
 * vertical bounds.
 */
const _appendTrailingColumnForScope = (
  dropzoneLayer: SVGElement,
  scope: LayoutScope,
  wireData: number[],
  minWire: number,
  maxWire: number,
  pathPrefix: string,
): void => {
  const trailingColIndex = scope.columnXOffsets.length;
  const ctx: DropzoneContext = { scope, wireData, pathPrefix };
  for (let wireIndex = minWire; wireIndex < maxWire; wireIndex++) {
    const dropzone = makeDropzoneBox(ctx, {
      colIndex: trailingColIndex,
      opIndex: 0,
      wireIndex,
      interColumn: true,
    });
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

  const ctx: DropzoneContext = { scope, wireData, pathPrefix };

  // The LayoutMap's column count matches the rendered grid (one entry
  // per processed column); using `Math.max` is just defensive against
  // unexpected mismatch.
  const colCount = Math.max(scope.columnXOffsets.length, grid.length);

  for (let colIndex = 0; colIndex < colCount; colIndex++) {
    const columnOps = grid[colIndex];
    if (columnOps == null) continue;

    // Precompute which wires this column's ops actually occupy.
    // A central dropzone at an occupied wire would visually sit on
    // top of a gate (or its connecting lines), even if the gate
    // belongs to a different op than the one being iterated — so
    // the "is this wire safe for a central drop?" question can't
    // be answered from a single op in isolation.
    //
    // We also need a per-wire `opIndex` for the dropzone's location
    // string. The action layer treats `opIndex` as the array
    // position to insert at (`Array.splice(opIndex, 0, op)`); the
    // renderer doesn't depend on array order for layout. So:
    //
    //   - Owned wire → use the owning op's opIndex. Drops "onto"
    //     the gate insert at the gate's array position.
    //   - Unowned wire → use `components.length`. Drops in a gap
    //     append to the column's array.
    //
    // Walk ops in declared order; first claimant of a wire wins
    // (overlapping ops in one column shouldn't occur — the action
    // layer's `_addOp` splits them into separate columns — but
    // defensive anyway).
    //
    // Quantum-only span: a classically-controlled op back-references
    // the producing measurement's qubit via `.controls`, but doesn't
    // render any body on that wire (only a small classical-control
    // circle sits on the row). Treating that wire as occupied would
    // suppress the central dropzone there, leaving the visually-empty
    // area at the group's column un-droppable for top-level inserts.
    const occupiedWires = new Set<number>();
    const wireOwnerOpIndex = new Map<number, number>();
    columnOps.components.forEach((op, opIndex) => {
      const [minT, maxT] = getQuantumWireRange(op);
      for (let w = minT; w <= maxT; w++) {
        occupiedWires.add(w);
        if (!wireOwnerOpIndex.has(w)) {
          wireOwnerOpIndex.set(w, opIndex);
        }
      }
    });

    // Wire-by-wire pass. The previous algorithm accumulated a
    // monotonically-increasing `wireIndex` across ops; that
    // assumed ops were sorted by `minTarget`, which the compiler
    // often violates (it tends to emit ops in execution order
    // rather than wire order). Iterating wires directly removes
    // that assumption and emits the same boxes for the
    // sorted-by-wire common case.
    for (let wireIndex = minWire; wireIndex < maxWire; wireIndex++) {
      const opIndex =
        wireOwnerOpIndex.get(wireIndex) ?? columnOps.components.length;

      // Inter-column band: always emit. It's a narrow vertical
      // strip on the left edge of the column ("insert a new column
      // before this one"); even when it slightly overlaps a gate's
      // body it doesn't visually conflict with the gate icon.
      dropzoneLayer.appendChild(
        makeDropzoneBox(ctx, {
          colIndex,
          opIndex,
          wireIndex,
          interColumn: true,
        }),
      );

      // Central full-width box: emit only at wires NOT occupied
      // by any op in this column. This is the fix for the phantom
      // dropzone bug — without the column-wide occupancy check, an
      // op's own "above-me" wires (`wireIndex < minTarget`) could
      // be occupied by a different op later in the column, and
      // the central box would sit on top of that op's gate.
      if (!occupiedWires.has(wireIndex)) {
        dropzoneLayer.appendChild(
          makeDropzoneBox(ctx, {
            colIndex,
            opIndex,
            wireIndex,
            interColumn: false,
          }),
        );
      }
    }

    // Recurse into expanded children. Decoupled from the wire loop
    // above because recursion depends only on the op's identity
    // and wire extent, not on `wireIndex`.
    //
    // The recursion's wire extent matches the group's own
    // [minTarget, maxTarget] (inclusive), ensuring nested dropzones
    // can never escape the parent group. Drive the
    // is-this-expanded decision off the LayoutMap rather than
    // `isExpandedGroup(op)`: `op` here belongs to
    // `sqore.circuit.componentGrid` (the original), while expand
    // flags from `expandOperationsToDepth`,
    // `expandIfSingleOperation`, and the user's expand-chevron
    // clicks are applied to the per-render deep copy only — never
    // to the original. The LayoutMap, built from that deep copy,
    // is the authoritative record of which groups were rendered
    // expanded.
    columnOps.components.forEach((op, opIndex) => {
      const childKey = composeLocation(pathPrefix, colIndex, opIndex);
      if (op.children != null && layoutMap.scopes.has(childKey)) {
        // Quantum-only span: a classically-controlled group's
        // `.controls` carries the producing measurement's qubit as
        // a back-reference, but that qubit isn't a member wire of
        // the group. Including it here would make drops onto that
        // qubit (and adds from the toolbox) silently land inside
        // the group; the user has to shift-drag to extend the group
        // to a new wire.
        const [minTarget, maxTarget] = getQuantumWireRange(op);
        _populateDropzonesForGrid(
          dropzoneLayer,
          layoutMap,
          op.children,
          childKey,
          wireData,
          minTarget,
          maxTarget + 1,
        );
      }
    });
  }

  // Trailing-append column for this scope. At the top level this is
  // the "add a brand-new column past the rightmost" affordance; for
  // an expanded group it's the right-edge extend-sideways band that
  // mirrors the leading-column band emitted at `colIndex=0` of the
  // column loop above. Runs once per scope, after the column loop,
  // so it sits at the same recursion depth as the children walk.
  _appendTrailingColumnForScope(
    dropzoneLayer,
    scope,
    wireData,
    minWire,
    maxWire,
    pathPrefix,
  );
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
 * The layout context shared by every dropzone drawn for one scope:
 * the scope's geometry, the model-address prefix that scope maps to,
 * and the global wire-Y table. All three are invariant across the
 * wires and columns of a single scope, so a caller builds this once
 * and reuses it for each dropzone it emits.
 */
interface DropzoneContext {
  /**
   * Layout geometry for the scope. Source of truth — comes straight
   * from `LayoutMap.scopes.get(pathPrefix)`.
   */
  scope: LayoutScope;
  /** Wire Y positions in absolute svg coords (whole-circuit table). */
  wireData: number[];
  /**
   * Hierarchical scope prefix; `""` (the default) at top level.
   * Composed into `data-dropzone-location` so `findParentArray`
   * walks into the right `children` grid on drop.
   */
  pathPrefix?: string;
}

/**
 * The single cell a dropzone targets within its `DropzoneContext`,
 * plus which of the two dropzone shapes to draw there.
 */
interface DropzoneTarget {
  /**
   * Column the dropzone belongs to. May equal
   * `scope.columnXOffsets.length` to address the trailing-append
   * column past the rightmost.
   */
  colIndex: number;
  /** Operation index inside the column. */
  opIndex: number;
  /** Index into `wireData` for this dropzone's row. */
  wireIndex: number;
  /**
   * `true` for the narrow band straddling the left edge of
   * `colIndex`; `false` for the full-width box covering the column.
   */
  interColumn: boolean;
}

/**
 * Create a dropzone box element for `target` within the scope
 * described by `ctx`.
 */
const makeDropzoneBox = (
  ctx: DropzoneContext,
  target: DropzoneTarget,
): SVGElement => {
  const { scope, wireData, pathPrefix = "" } = ctx;
  const { colIndex, opIndex, wireIndex, interColumn } = target;
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

/**
 * Build the ghost-border `<rect>` that previews a group's extended
 * bounding box during a D4 Stage B shift+drag.
 *
 * The rect covers:
 *
 * - Horizontally: from the group's leftmost column's start x to its
 *   rightmost column's right edge. If `hoverColIndex` lies past the
 *   last column (the trailing-append column), the rect extends right
 *   to include that synthesized column too — so the user sees the
 *   group's new horizontal footprint along with the new vertical
 *   one when the drop is on the trailing column.
 * - Vertically: from `min(top wire Y, hover wire Y)` to
 *   `max(bottom wire Y, hover wire Y)`, padded by `DROPZONE_PADDING_Y`
 *   on each side so the ghost reads as a generous halo around the
 *   group's body rather than a tight stripe over the wires.
 *
 * Coordinates come entirely from `LayoutScope` + `wireData`, the
 * same sources Stage A's dropzones use. No DOM lookup of the
 * group's rendered `<rect>` — that would couple the overlay to
 * `gateFormatter`'s internal structure.
 *
 * Caller appends the returned element to `overlayLayer` and removes
 * it on hover-off / shift-release / mouseup.
 */
const makeShiftExtendGhost = (
  scope: LayoutScope,
  wireData: number[],
  groupMinWire: number,
  groupMaxWire: number,
  hoverWireIndex: number,
  hoverColIndex: number,
): SVGElement => {
  // Horizontal: leftmost column start → rightmost column right edge.
  // The trailing-append case (hoverColIndex past the last real
  // column) extends right via `columnGeometry`'s synthesized position
  // so the hover column gets covered too.
  const leftGeom = columnGeometry(scope, 0);
  const lastRealColIndex = Math.max(scope.columnXOffsets.length - 1, 0);
  const rightRealGeom = columnGeometry(scope, lastRealColIndex);
  const rightRealEdge = rightRealGeom.colStartX + rightRealGeom.colWidth;
  const rightTrailGeom = columnGeometry(scope, scope.columnXOffsets.length);
  const rightEdge =
    hoverColIndex >= scope.columnXOffsets.length
      ? rightTrailGeom.colStartX + rightTrailGeom.colWidth
      : rightRealEdge;

  // Vertical: pull in the existing wire span plus the hovered wire,
  // and pad. We index `wireData` defensively in case `hoverWireIndex`
  // is the trailing ghost-qubit row (length == wireData.length); fall
  // back to the last real wire if so, since extending onto the ghost
  // row isn't a supported action.
  const topWireY = wireData[groupMinWire] ?? wireData[0];
  const bottomWireY = wireData[groupMaxWire] ?? wireData[wireData.length - 1];
  const hoverWireY = wireData[hoverWireIndex] ?? wireData[wireData.length - 1];
  const topY = Math.min(topWireY, hoverWireY) - DROPZONE_PADDING_Y;
  const bottomY = Math.max(bottomWireY, hoverWireY) + DROPZONE_PADDING_Y;

  return box(
    leftGeom.colStartX - gatePadding,
    topY,
    rightEdge - leftGeom.colStartX + gatePadding * 2,
    bottomY - topY,
    "shift-extend-ghost",
  );
};

export {
  createDropzones,
  createGateGhost,
  createQubitLabelGhost,
  createWireDropzone,
  makeDropzoneBox,
  makeShiftExtendGhost,
  removeAllWireDropzones,
};
