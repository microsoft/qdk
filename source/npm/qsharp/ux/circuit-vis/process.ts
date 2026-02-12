// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import {
  minGateWidth,
  startX,
  gatePadding,
  controlBtnOffset,
  groupPaddingX,
  groupTopPadding,
  groupBottomPadding,
} from "./constants.js";
import {
  ComponentGrid,
  Operation,
  ConditionalRender,
  SourceLocation,
} from "./circuit.js";
import { GateRenderData, GateType } from "./gateRenderData.js";
import { Register, RegisterMap } from "./register.js";
import { getMinGateWidth } from "./utils.js";

/**
 * Takes in a component grid and maps the operations to `GateRenderData` objects which
 * contains information for formatting the corresponding SVG.
 *
 * @param componentGrid Grid of circuit components.
 * @param registers  Mapping from qubit IDs to register render data.
 * @param topY y-coordinate of the topmost register involved in the operation.
 * @param bottomY y-coordinate of the bottommost register involved in the operation.
 * @param renderLocations Optional function to map source locations to link hrefs and titles.
 *
 * @returns An object containing `renderDataArray` (2D Array of GateRenderData objects) and
 *          `svgWidth` which is the width of the entire SVG.
 */
const processOperations = (
  componentGrid: ComponentGrid,
  topY: number,
  bottomY: number,
  registers: RegisterMap,
  renderLocations?: (s: SourceLocation[]) => { title: string; href: string },
): {
  renderDataArray: GateRenderData[][];
  svgWidth: number;
  maxTopPadding: number;
  maxBottomPadding: number;
} => {
  if (componentGrid.length === 0)
    return {
      renderDataArray: [],
      svgWidth: startX + gatePadding * 2,
      maxTopPadding: 0,
      maxBottomPadding: 0,
    };
  const numColumns: number = componentGrid.length;
  const columnsWidths: number[] = new Array(numColumns).fill(minGateWidth);

  let maxTopPadding = 0;
  let maxBottomPadding = 0;

  // Get classical registers and their starting column index
  const classicalRegs: [number, Register][] =
    _getClassicalRegStarts(componentGrid);

  // Map operation index to gate render data for formatting later
  const renderDataArray: GateRenderData[][] = componentGrid.map(
    (col, colIndex) =>
      col.components.map((op) => {
        const renderData: GateRenderData = _opToRenderData(
          op,
          registers,
          renderLocations,
        );

        let targets: Register[];
        switch (op.kind) {
          case "unitary":
            targets = op.targets;
            break;
          case "measurement":
            targets = op.qubits;
            break;
          case "ket":
            targets = op.targets;
            break;
        }
        const minTargetY = Math.min(...(renderData.targetsY as number[]));
        const maxTargetY = Math.max(...(renderData.targetsY as number[]));

        if (topY === minTargetY) {
          maxTopPadding = Math.max(maxTopPadding, renderData.topPadding);
        }
        if (bottomY === maxTargetY) {
          maxBottomPadding = Math.max(
            maxBottomPadding,
            renderData.bottomPadding,
          );
        }

        if (
          op != null &&
          [GateType.Unitary, GateType.Ket, GateType.ControlledUnitary].includes(
            renderData.type,
          )
        ) {
          // If gate is a unitary type, split targetsY into groups if there
          // is a classical register between them for rendering

          // Get y coordinates of classical registers in the same column as this operation
          const classicalRegY: number[] = classicalRegs
            .filter(([regCol]) => regCol <= colIndex)
            .map(([, reg]) => {
              if (reg.result == null)
                throw new Error("Could not find cId for classical register.");
              const { children } = registers[reg.qubit];
              if (children == null)
                throw new Error(
                  `Failed to find classical registers for qubit ID ${reg.qubit}.`,
                );
              return children[reg.result].y;
            });

          renderData.targetsY = _splitTargetsY(
            targets,
            classicalRegY,
            registers,
          );
        }

        // Expand column size, if needed
        if (renderData.width > columnsWidths[colIndex]) {
          columnsWidths[colIndex] = renderData.width;
        }

        return renderData;
      }),
  );

  // Filter out invalid gates
  const filteredArray: GateRenderData[][] = renderDataArray
    .map((col) => col.filter(({ type }) => type != GateType.Invalid))
    .filter((col) => col.length > 0);

  // Fill in x coord of each gate
  const endX: number = _fillRenderDataX(filteredArray, columnsWidths);

  return {
    renderDataArray: filteredArray,
    svgWidth: endX,
    maxTopPadding,
    maxBottomPadding,
  };
};

/**
 * Retrieves the starting index of each classical register.
 *
 * @param componentGrid Grid of circuit components.
 *
 * @returns Array of classical register and their starting column indices in the form [[column, register]].
 */
const _getClassicalRegStarts = (
  componentGrid: ComponentGrid,
): [number, Register][] => {
  const clsRegs: [number, Register][] = [];
  componentGrid.forEach((col, colIndex) => {
    col.components.forEach((op) => {
      if (op.kind === "measurement") {
        const resultRegs: Register[] = op.results.filter(
          ({ result }) => result !== undefined,
        );
        resultRegs.forEach((reg) => clsRegs.push([colIndex, reg]));
      } else if (op.children != null) {
        const componentGrid = op.children;
        const childClsRegs = _getClassicalRegStarts(componentGrid);
        childClsRegs.forEach(([, reg]) => {
          clsRegs.push([colIndex, reg]);
        });
      }
    });
  });
  return clsRegs;
};

/**
 * Maps operation to render data (e.g. gate type, position, dimensions, text)
 * required to render the image.
 *
 * @param op        Operation to be mapped into render data.
 * @param registers Array of registers.
 *
 * @returns GateRenderData representation of given operation.
 */
const _opToRenderData = (
  op: Operation | null,
  registers: RegisterMap,
  renderLocations?: (s: SourceLocation[]) => { title: string; href: string },
): GateRenderData => {
  const renderData: GateRenderData = {
    type: GateType.Invalid,
    x: 0,
    controlsY: [],
    targetsY: [],
    label: "",
    width: -1,
    topPadding: 0,
    bottomPadding: 0,
  };

  if (op == null) return renderData;

  let isAdjoint: boolean;
  let controls: Register[] | undefined;
  let targets: Register[];
  switch (op.kind) {
    case "measurement":
      isAdjoint = false;
      controls = op.qubits;
      targets = op.results;
      break;
    case "unitary":
      isAdjoint = op.isAdjoint ?? false;
      controls = op.controls;
      targets = op.targets;
      break;
    case "ket":
      isAdjoint = false;
      controls = [];
      targets = op.targets;
      break;
  }

  const {
    gate,
    args,
    children,
    dataAttributes,
    isConditional,
    conditionalRender,
  } = op;

  // Set y coords
  renderData.controlsY = controls?.map((reg) => _getRegY(reg, registers)) || [];
  renderData.targetsY = targets.map((reg) => _getRegY(reg, registers));

  if (isConditional) {
    // Classically-controlled operations
    if (children == null || children.length == 0)
      throw new Error(
        "No children operations found for classically-controlled operation.",
      );

    renderData.type = GateType.ClassicalControlled;
    renderData.label = gate;

    _processChildren(renderData, children, registers, renderLocations);

    // Fill in the ID to be displayed in each control wire's circle.
    renderData.classicalControlIds =
      controls
        ?.map(
          (reg) =>
            op.metadata?.controlResultIds?.find(
              (e) => e[0].qubit === reg.qubit && e[0].result === reg.result,
            )?.[1],
        )
        .map((id) => id ?? null) || [];

    // Add additional width for classical control circle
    renderData.width += controlBtnOffset;
  } else if (
    conditionalRender == ConditionalRender.AsGroup &&
    children &&
    (children.length || 0) > 0
  ) {
    // Grouped operations

    renderData.type = GateType.Group;
    // _zoomButton function in gateFormatter.ts relies on
    // 'expanded' attribute to render zoom button
    renderData.dataAttributes = { expanded: "true" };
    renderData.label = gate;

    _processChildren(renderData, children, registers, renderLocations);
  } else if (op.kind === "measurement") {
    renderData.type = GateType.Measure;
  } else if (op.kind === "ket") {
    renderData.type = GateType.Ket;
    renderData.label = gate;
  } else if (gate === "SWAP") {
    renderData.type = GateType.Swap;
  } else if (controls && controls.length > 0) {
    renderData.type = gate === "X" ? GateType.Cnot : GateType.ControlledUnitary;
    renderData.label = gate;
  } else if (gate === "X") {
    renderData.type = GateType.X;
    renderData.label = gate;
  } else {
    // Any other gate treated as a simple unitary gate
    renderData.type = GateType.Unitary;
    renderData.label = gate;
  }

  // If adjoint, add ' to the end of gate label
  if (isAdjoint && renderData.label.length > 0) renderData.label += "'";

  // If gate has extra arguments, display them
  // For now, we only display the first argument
  if (args !== undefined && args.length > 0) renderData.displayArgs = args[0];

  // Minimum width is calculated based on the label and args
  const minWidth = getMinGateWidth(renderData);
  renderData.width = Math.max(minWidth, renderData.width);

  if (op.metadata?.source && renderLocations) {
    renderData.link = renderLocations([op.metadata.source]);
  }

  // Extend existing data attributes with user-provided data attributes
  if (dataAttributes != null)
    renderData.dataAttributes = {
      ...renderData.dataAttributes,
      ...dataAttributes,
    };

  return renderData;
};

/**
 * Compute the y coord of a given register.
 *
 * @param reg       Register to compute y coord of.
 * @param registers Map of qubit IDs to RegisterRenderData.
 *
 * @returns The y coord of give register.
 */
const _getRegY = (reg: Register, registers: RegisterMap): number => {
  const { qubit: qId, result } = reg;
  if (!Object.prototype.hasOwnProperty.call(registers, qId))
    throw new Error(`ERROR: Qubit register with ID ${qId} not found.`);
  const { y, children } = registers[qId];
  if (result == null) {
    return y;
  } else {
    if (children == null)
      throw new Error(
        `ERROR: No classical registers found for qubit ID ${qId}.`,
      );
    if (children.length <= result)
      throw new Error(
        `ERROR: Classical register ID ${result} invalid for qubit ID ${qId} with ${children.length} classical register(s).`,
      );
    return children[result].y;
  }
};

/**
 * Splits `targets` if non-adjacent or intersected by classical registers.
 *
 * @param targets       Target registers (can be qubit or classical).
 * @param classicalRegY y coords of classical registers overlapping current column.
 * @param registers     Mapping from register qubit IDs to register render data.
 *
 * @returns Groups of target qubit y coords.
 */
const _splitTargetsY = (
  targets: Register[],
  classicalRegY: number[],
  registers: RegisterMap,
): number[][] => {
  if (targets.length === 0) return [];

  // Get qIds sorted by ascending y value
  const orderedQIds: number[] = Object.keys(registers).map(Number);
  orderedQIds.sort((a, b) => registers[a].y - registers[b].y);
  const qIdPosition: { [qId: number]: number } = {};
  orderedQIds.forEach((qId, i) => (qIdPosition[qId] = i));

  // Sort targets and classicalRegY by ascending y value
  targets = targets.slice();
  targets.sort((a, b) => {
    const posDiff: number = qIdPosition[a.qubit] - qIdPosition[b.qubit];
    if (posDiff === 0 && a.result != null && b.result != null)
      return a.result - b.result;
    else return posDiff;
  });
  classicalRegY = classicalRegY.slice();
  classicalRegY.sort((a, b) => a - b);

  let prevPos = 0;
  let prevY = 0;

  return targets.reduce((groups: number[][], target: Register) => {
    const y = _getRegY(target, registers);
    const pos = qIdPosition[target.qubit];

    // Split into new group if one of the following holds:
    //      1. First target register
    //      2. Non-adjacent qubit registers
    //      3. There is a classical register between current and previous register
    if (
      groups.length === 0 ||
      pos > prevPos + 1 ||
      (classicalRegY[0] > prevY && classicalRegY[0] < y)
    )
      groups.push([y]);
    else groups[groups.length - 1].push(y);

    prevPos = pos;
    prevY = y;

    // Remove classical registers that are higher than current y
    while (classicalRegY.length > 0 && classicalRegY[0] <= y)
      classicalRegY.shift();

    return groups;
  }, []);
};

/**
 * Updates the x coord of each render data object in the given 2D array and returns rightmost x coord.
 *
 * @param renderDataArray  2D array of render data.
 * @param columnWidths Array of column widths.
 *
 * @returns Rightmost x coord.
 */
const _fillRenderDataX = (
  renderDataArray: GateRenderData[][],
  columnWidths: number[],
): number => {
  let endX: number = startX;

  const colStartX: number[] = columnWidths.map((width) => {
    const x: number = endX;
    endX += width + gatePadding * 2;
    return x;
  });

  renderDataArray.forEach((col, colIndex) =>
    col.forEach((renderData) => {
      const x = colStartX[colIndex];
      switch (renderData.type) {
        case GateType.ClassicalControlled:
        case GateType.Group:
          {
            // Subtract startX offset from nested gates and add offset and padding
            let offset: number = x - startX + groupPaddingX;
            if (renderData.type === GateType.ClassicalControlled) {
              offset += controlBtnOffset;
            }

            // Offset each x coord in children gates
            _offsetChildrenX(renderData.children, offset);

            // Groups should be left-aligned in their column
            renderData.x = x + renderData.width / 2;
          }
          break;

        default:
          // Center gate in column
          renderData.x = x + columnWidths[colIndex] / 2;
          break;
      }
    }),
  );

  return endX + gatePadding;
};

/**
 * Offset x coords of nested children operations.
 *
 * @param children 2D or 3D array of children GateRenderData.
 * @param offset   x coord offset.
 */
const _offsetChildrenX = (
  children: GateRenderData[][] | GateRenderData[][][] | undefined,
  offset: number,
): void => {
  if (children == null) return;
  children.forEach((col) => {
    col.flat().forEach((child) => {
      child.x += offset;
      _offsetChildrenX(child.children, offset);
    });
  });
};

/**
 * Processes the children operations and updates the render data accordingly.
 *
 * @param renderData        Render data of the parent operation, to be updated with children data.
 * @param children          Nested operations to be processed.
 * @param registers         Mapping from qubit IDs to register render data.
 * @param renderLocations   Optional function to map source locations to link hrefs and titles
 */
function _processChildren(
  renderData: GateRenderData,
  children: ComponentGrid,
  registers: RegisterMap,
  renderLocations?: (s: SourceLocation[]) => { title: string; href: string },
) {
  const topY = Math.min(...(renderData.targetsY as number[]));
  const bottomY = Math.max(...(renderData.targetsY as number[]));

  const childrenInstrs = processOperations(
    children,
    topY,
    bottomY,
    registers,
    renderLocations,
  );

  renderData.children = childrenInstrs.renderDataArray;
  renderData.width =
    childrenInstrs.svgWidth - startX - gatePadding * 3 + groupPaddingX * 2; // (svgWidth includes 3 extra gate paddings)
  renderData.topPadding = childrenInstrs.maxTopPadding + groupTopPadding;
  renderData.bottomPadding =
    childrenInstrs.maxBottomPadding + groupBottomPadding;
}

export { processOperations };
