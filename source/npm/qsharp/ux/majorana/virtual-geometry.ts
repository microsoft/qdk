// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import type { MajoranaGeometry, Point, Rectangle } from "./geometry.js";
import type {
  VirtualAdjacencyEdge,
  VirtualQubit,
  VirtualTopology,
} from "./virtual-topology.js";

export type VirtualQubitGeometry = {
  virtualQubitId: number;
  center: Point;
  radius: number;
  bounds: Rectangle;
};

export type VirtualConnectorGeometry = {
  edgeId: string;
  orientation: VirtualAdjacencyEdge["orientation"];
  scope: VirtualAdjacencyEdge["scope"];
  supportedOperation: VirtualAdjacencyEdge["supportedOperation"];
  start: Point;
  end: Point;
};

export type VirtualGeometry = {
  viewBox: Rectangle;
  qubits: ReadonlyMap<number, VirtualQubitGeometry>;
  adjacencyEdges: ReadonlyMap<string, VirtualConnectorGeometry>;
};

export const VIRTUAL_GEOMETRY = {
  qubitRadius: 72,
  labelBaselineOffset: -40,
  operationBaselineOffset: 10,
  connectorWidth: 8,
  activeConnectorWidth: 14,
} as const;

export function createVirtualGeometry(
  topology: VirtualTopology,
  physicalGeometry: MajoranaGeometry,
): VirtualGeometry {
  const qubits = new Map<number, VirtualQubitGeometry>();
  const adjacencyEdges = new Map<string, VirtualConnectorGeometry>();

  for (const qubit of topology.qubits) {
    const center = virtualQubitCenter(qubit, physicalGeometry);
    const radius = VIRTUAL_GEOMETRY.qubitRadius;
    qubits.set(qubit.id, {
      virtualQubitId: qubit.id,
      center,
      radius,
      bounds: {
        x: center.x - radius,
        y: center.y - radius,
        width: radius * 2,
        height: radius * 2,
      },
    });
  }

  for (const edge of topology.adjacencyEdges) {
    const first = requiredVirtualQubitGeometry(
      qubits,
      edge.firstVirtualQubitId,
    );
    const second = requiredVirtualQubitGeometry(
      qubits,
      edge.secondVirtualQubitId,
    );
    adjacencyEdges.set(edge.id, {
      edgeId: edge.id,
      orientation: edge.orientation,
      scope: edge.scope,
      supportedOperation: edge.supportedOperation,
      start: connectorEndpoint(first, second.center),
      end: connectorEndpoint(second, first.center),
    });
  }

  return {
    viewBox: { ...physicalGeometry.viewBox },
    qubits,
    adjacencyEdges,
  };
}

function virtualQubitCenter(
  qubit: VirtualQubit,
  physicalGeometry: MajoranaGeometry,
): Point {
  const first = requiredPhysicalQubitCenter(
    physicalGeometry,
    qubit.physicalQubitIds[0],
  );
  const second = requiredPhysicalQubitCenter(
    physicalGeometry,
    qubit.physicalQubitIds[1],
  );
  return {
    x: (first.x + second.x) / 2,
    y: (first.y + second.y) / 2,
  };
}

function connectorEndpoint(
  qubit: VirtualQubitGeometry,
  otherCenter: Point,
): Point {
  const deltaX = otherCenter.x - qubit.center.x;
  const deltaY = otherCenter.y - qubit.center.y;
  const distance = Math.hypot(deltaX, deltaY);
  if (distance === 0) {
    throw new Error(
      `Cannot create a virtual connector from V${qubit.virtualQubitId} to itself`,
    );
  }
  return {
    x: qubit.center.x + (deltaX / distance) * qubit.radius,
    y: qubit.center.y + (deltaY / distance) * qubit.radius,
  };
}

function requiredPhysicalQubitCenter(
  geometry: MajoranaGeometry,
  qubitId: number,
): Point {
  const tetron = geometry.tetrons.get(qubitId);
  if (tetron === undefined) {
    throw new Error(`Missing physical geometry for Q${qubitId}`);
  }
  return tetron.center;
}

function requiredVirtualQubitGeometry(
  qubits: ReadonlyMap<number, VirtualQubitGeometry>,
  virtualQubitId: number,
): VirtualQubitGeometry {
  const qubit = qubits.get(virtualQubitId);
  if (qubit === undefined) {
    throw new Error(`Missing virtual geometry for V${virtualQubitId}`);
  }
  return qubit;
}
