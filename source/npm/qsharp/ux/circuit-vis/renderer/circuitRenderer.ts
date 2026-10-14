// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { formatGates } from "./formatters/gateFormatter.js";
import { formatInputs } from "./formatters/inputFormatter.js";
import { formatRegisters } from "./formatters/registerFormatter.js";
import { GateRenderData } from "./gateRenderData.js";
import { emptyLayoutMap, LayoutMap } from "./layoutMap.js";
import { processOperations } from "./process.js";
import { createSvgElement, SvgElement } from "./svg.js";
import {
  Circuit,
  ComponentGrid,
  Operation,
  Qubit,
  SourceLocation,
} from "../data/circuit.js";
import { Location } from "../data/location.js";
import { getOperationRegisters } from "../utils.js";
import type { CircuitSvgExpansion } from "./circuitSvgOptions.js";

type RenderLocations = (locations: SourceLocation[]) => {
  title: string;
  href: string;
};

export type CircuitRenderOptions = {
  renderDepth?: number;
  expansion?: CircuitSvgExpansion | "current";
  applyCurrentExpansion?: (grid: ComponentGrid) => void;
  renderLocations?: RenderLocations;
};

export type RenderedCircuit = {
  svg: SvgElement;
  layoutMap: LayoutMap;
};

type ComposedCircuit = {
  width: number;
  height: number;
  elements: SvgElement[];
  layoutMap: LayoutMap;
};

type RowHeight = {
  currentGroupBordersAboveWire: number;
  currentGroupBordersBelowWire: number;
  currentClassicalGroupsAboveFirstClassical: number;
  currentBottomBordersAboveFirstClassical: number;
  heightAboveWire: number;
  heightBelowWire: number;
  heightAboveFirstClassical: number;
  bottomBordersAboveFirstClassical: number;
};

type RowHeights = Record<number, RowHeight>;

export function renderCircuit(
  source: Circuit,
  options: CircuitRenderOptions = {},
): RenderedCircuit {
  const circuit = prepareCircuit(source, options);
  const composed = composeCircuit(circuit, options.renderLocations);
  return {
    svg: createCircuitSvg(composed),
    layoutMap: composed.layoutMap,
  };
}

function prepareCircuit(
  source: Circuit,
  options: CircuitRenderOptions,
): Circuit {
  const circuit: Circuit = JSON.parse(JSON.stringify(source));
  const renderDepth = options.renderDepth ?? 0;
  const expansion = options.expansion ?? "model";

  assignOperationLocations(circuit.componentGrid, Location.root());
  expandOperationsToDepth(circuit.componentGrid, renderDepth);
  expandIfSingleOperation(circuit.componentGrid);

  if (expansion === "current") {
    options.applyCurrentExpansion?.(circuit.componentGrid);
  } else if (expansion === "collapsed") {
    setExpansion(circuit.componentGrid, false);
  } else if (expansion === "expanded") {
    setExpansion(circuit.componentGrid, true);
  }

  return circuit;
}

function assignOperationLocations(grid: ComponentGrid, parent: Location): void {
  grid.forEach((column, columnIndex) =>
    column.components.forEach((operation, operationIndex) => {
      const location = parent.child(columnIndex, operationIndex);
      operation.dataAttributes ??= {};
      operation.dataAttributes["location"] = location.toString();
      if (operation.children != null) {
        assignOperationLocations(operation.children, location);
      }
    }),
  );
}

function expandOperationsToDepth(
  componentGrid: ComponentGrid,
  targetDepth: number,
  currentDepth: number = 0,
): void {
  for (const column of componentGrid) {
    for (const operation of column.components) {
      if (currentDepth < targetDepth && operation.children != null) {
        operation.dataAttributes ??= {};
        operation.dataAttributes["expanded"] = "true";
        expandOperationsToDepth(
          operation.children,
          targetDepth,
          currentDepth + 1,
        );
      }
    }
  }
}

function expandIfSingleOperation(grid: ComponentGrid): void {
  if (grid.length === 1 && grid[0].components.length === 1) {
    const onlyComponent = grid[0].components[0];
    if (
      onlyComponent.dataAttributes != null &&
      Object.prototype.hasOwnProperty.call(
        onlyComponent.dataAttributes,
        "location",
      ) &&
      onlyComponent.dataAttributes["expanded"] !== "false" &&
      onlyComponent.children != null
    ) {
      onlyComponent.dataAttributes["expanded"] = "true";
    }
  }

  for (const column of grid) {
    for (const operation of column.components) {
      expandIfSingleOperation(operation.children ?? []);
    }
  }
}

function setExpansion(grid: ComponentGrid, expanded: boolean): void {
  for (const column of grid) {
    for (const operation of column.components) {
      if (operation.children != null) {
        operation.dataAttributes ??= {};
        operation.dataAttributes["expanded"] = expanded ? "true" : "false";
        setExpansion(operation.children, expanded);
      }
    }
  }
}

function composeCircuit(
  circuit: Circuit,
  renderLocations: RenderLocations | undefined,
): ComposedCircuit {
  const { qubits, componentGrid } = circuit;
  const rowHeights = getRowHeights(qubits, componentGrid);
  const { qubitLabels, registers, svgHeight } = formatInputs(
    qubits,
    rowHeights,
    renderLocations,
  );
  const topY = qubits[0] ? registers[qubits[0].id].y : -1;
  const bottomY = qubits[qubits.length - 1]
    ? registers[qubits[qubits.length - 1].id].y
    : -1;
  const { renderDataArray, svgWidth, localScope, childScopes } =
    processOperations(componentGrid, topY, bottomY, registers, renderLocations);

  const layoutMap = emptyLayoutMap();
  layoutMap.scopes.set("", localScope);
  for (const [key, scope] of childScopes) {
    layoutMap.scopes.set(key, scope);
  }
  layoutMap.wireYs = qubits.map((qubit) => registers[qubit.id].y);

  return {
    width: svgWidth,
    height: svgHeight,
    elements: [
      qubitLabels,
      formatRegisters(registers, flattenRenderData(renderDataArray), svgWidth),
      formatGates(renderDataArray),
    ],
    layoutMap,
  };
}

function flattenRenderData(renderData: GateRenderData[][]): GateRenderData[] {
  const result: GateRenderData[] = [];
  const add = (gate: GateRenderData | GateRenderData[]): void => {
    if (Array.isArray(gate)) {
      gate.forEach(add);
    } else {
      result.push(gate);
      gate.children?.forEach((column) => column.forEach(add));
    }
  };
  renderData.forEach((column) => column.forEach(add));
  return result;
}

function createCircuitSvg(composed: ComposedCircuit): SvgElement {
  const svg = createSvgElement("svg", {
    class: "qviz",
    width: composed.width.toString(),
    height: composed.height.toString(),
  });
  composed.elements.forEach((element) => svg.appendChild(element));
  svg.setAttribute("viewBox", `0 0 ${composed.width} ${composed.height}`);
  return svg;
}

/**
 * Recursively computes vertical space required to render group borders.
 */
function getRowHeights(
  qubits: Qubit[],
  componentGrid: ComponentGrid,
): RowHeights {
  const rowHeights: RowHeights = {};
  const numResultsByQubit: Record<number, number> = {};

  for (const qubit of qubits) {
    rowHeights[qubit.id] = {
      currentGroupBordersBelowWire: 0,
      currentGroupBordersAboveWire: 0,
      currentClassicalGroupsAboveFirstClassical: 0,
      currentBottomBordersAboveFirstClassical: 0,
      heightBelowWire: 0,
      heightAboveWire: 0,
      heightAboveFirstClassical: 0,
      bottomBordersAboveFirstClassical: 0,
    };
    numResultsByQubit[qubit.id] = qubit.numResults ?? 0;
  }

  updateRowHeights(componentGrid, rowHeights, numResultsByQubit);
  return rowHeights;
}

function updateRowHeights(
  componentGrid: ComponentGrid,
  rowHeights: RowHeights,
  numResultsByQubit: Readonly<Record<number, number>>,
): void {
  for (const column of componentGrid) {
    for (const component of column.components) {
      if (!isExpandedGroup(component)) {
        continue;
      }

      const registers = getOperationRegisters(component);
      if (registers.length === 0) {
        continue;
      }

      const qubits = registers.map((register) => register.qubit);
      const minQubit = Math.min(...qubits);
      const maxQubit = Math.max(...qubits);
      const minQubitHasPureRef = registers.some(
        (register) => register.qubit === minQubit && register.result == null,
      );
      const maxQubitHasClassicalRef = registers.some(
        (register) => register.qubit === maxQubit && register.result != null,
      );

      let bumpedAboveWireQ: number | null = null;
      let bumpedTopFirstClassicalQ: number | null = null;
      let bumpedBottomFirstClassicalQ: number | null = null;
      let bumpedBelowWireQ: number | null = null;

      if (minQubitHasPureRef) {
        rowHeights[minQubit].currentGroupBordersAboveWire++;
        rowHeights[minQubit].heightAboveWire = Math.max(
          rowHeights[minQubit].heightAboveWire,
          rowHeights[minQubit].currentGroupBordersAboveWire,
        );
        bumpedAboveWireQ = minQubit;
      } else {
        rowHeights[minQubit].currentClassicalGroupsAboveFirstClassical++;
        rowHeights[minQubit].heightAboveFirstClassical = Math.max(
          rowHeights[minQubit].heightAboveFirstClassical,
          rowHeights[minQubit].currentClassicalGroupsAboveFirstClassical,
        );
        bumpedTopFirstClassicalQ = minQubit;
      }

      if (!maxQubitHasClassicalRef && (numResultsByQubit[maxQubit] ?? 0) > 0) {
        rowHeights[maxQubit].currentBottomBordersAboveFirstClassical++;
        rowHeights[maxQubit].bottomBordersAboveFirstClassical = Math.max(
          rowHeights[maxQubit].bottomBordersAboveFirstClassical,
          rowHeights[maxQubit].currentBottomBordersAboveFirstClassical,
        );
        bumpedBottomFirstClassicalQ = maxQubit;
      } else {
        rowHeights[maxQubit].currentGroupBordersBelowWire++;
        rowHeights[maxQubit].heightBelowWire = Math.max(
          rowHeights[maxQubit].heightBelowWire,
          rowHeights[maxQubit].currentGroupBordersBelowWire,
        );
        bumpedBelowWireQ = maxQubit;
      }

      updateRowHeights(component.children ?? [], rowHeights, numResultsByQubit);

      if (bumpedAboveWireQ != null) {
        rowHeights[bumpedAboveWireQ].currentGroupBordersAboveWire--;
      }
      if (bumpedTopFirstClassicalQ != null) {
        rowHeights[bumpedTopFirstClassicalQ]
          .currentClassicalGroupsAboveFirstClassical--;
      }
      if (bumpedBottomFirstClassicalQ != null) {
        rowHeights[bumpedBottomFirstClassicalQ]
          .currentBottomBordersAboveFirstClassical--;
      }
      if (bumpedBelowWireQ != null) {
        rowHeights[bumpedBelowWireQ].currentGroupBordersBelowWire--;
      }
    }
  }
}

function isExpandedGroup(component: Operation): boolean {
  const expanded = component.dataAttributes?.["expanded"];
  if (expanded != null) {
    return expanded === "true";
  }

  const hasChildren =
    component.children != null && component.children.length > 0;
  const hasClassicalControls =
    component.kind === "unitary" &&
    ((component.controls ?? []).some((register) => register.result != null) ||
      (component.metadata?.controlResultIds?.length ?? 0) > 0);

  return hasChildren && hasClassicalControls;
}
