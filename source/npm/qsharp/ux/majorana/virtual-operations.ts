// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import type {
  MajoranaOperationName,
  ValidatedMajoranaOperation,
  ValidatedMajoranaTraceStep,
  ValidatedVirtualOperation,
  VirtualOperationName,
} from "./types.js";
import {
  getVirtualAdjacencyEdge,
  type VirtualAdjacencyEdge,
  type VirtualTopology,
} from "./virtual-topology.js";

export type VirtualOperationIdentity = {
  key: string;
  name: VirtualOperationName;
  normalizedTargetIds: readonly number[];
};

export type ContributingPhysicalEvent = {
  operation: MajoranaOperationName;
  targetQubitIds: readonly number[];
};

export type VirtualGlyph =
  | {
      virtualQubitId: number;
      text: "T" | "H" | "S" | "⟨X❘" | "⟨Y❘" | "⟨Z❘";
    }
  | {
      virtualQubitId: number;
      text: "●";
      role: "control" | "participant";
    }
  | {
      virtualQubitId: number;
      text: "⨁";
      role: "target";
    };

export type VirtualOperationProjection = {
  identity: VirtualOperationIdentity;
  operation: ValidatedVirtualOperation;
  glyphs: readonly VirtualGlyph[];
  activeAdjacencyEdgeId: string | undefined;
  contributingPhysicalEvents: readonly ContributingPhysicalEvent[];
};

export function createVirtualOperationIdentity(
  operation: ValidatedVirtualOperation,
): VirtualOperationIdentity {
  const normalizedTargetIds = normalizedVirtualTargetIds(operation);
  return {
    key: JSON.stringify([operation.name, normalizedTargetIds]),
    name: operation.name,
    normalizedTargetIds,
  };
}

export function projectVirtualOperation(
  topology: VirtualTopology,
  operation: ValidatedVirtualOperation,
  contributingPhysicalEvents: readonly ContributingPhysicalEvent[] = [],
): VirtualOperationProjection {
  const identity = createVirtualOperationIdentity(operation);
  if (operation.kind === "single") {
    requireVirtualQubit(topology, operation.target);
    return {
      identity,
      operation,
      glyphs: [
        {
          virtualQubitId: operation.target,
          text: singleVirtualGlyph(operation.name),
        },
      ],
      activeAdjacencyEdgeId: undefined,
      contributingPhysicalEvents: [...contributingPhysicalEvents],
    };
  }

  const firstTarget =
    operation.name === "CX" ? operation.control : operation.firstTarget;
  const secondTarget =
    operation.name === "CX" ? operation.target : operation.secondTarget;
  const edge = requireVirtualAdjacency(
    topology,
    operation,
    firstTarget,
    secondTarget,
  );
  if (operation.name === "CX") {
    return {
      identity,
      operation,
      glyphs: [
        {
          virtualQubitId: operation.control,
          text: "●",
          role: "control",
        },
        {
          virtualQubitId: operation.target,
          text: "⨁",
          role: "target",
        },
      ],
      activeAdjacencyEdgeId: edge.id,
      contributingPhysicalEvents: [...contributingPhysicalEvents],
    };
  }

  return {
    identity,
    operation,
    glyphs: identity.normalizedTargetIds.map((virtualQubitId) => ({
      virtualQubitId,
      text: "●",
      role: "participant",
    })),
    activeAdjacencyEdgeId: edge.id,
    contributingPhysicalEvents: [...contributingPhysicalEvents],
  };
}

export function projectVirtualTraceStep(
  topology: VirtualTopology,
  step: ValidatedMajoranaTraceStep,
): readonly VirtualOperationProjection[] {
  const projections = new Map<string, VirtualOperationProjection>();
  for (const physicalOperation of step) {
    const virtualOperation = physicalOperation.virtualOperation;
    if (virtualOperation === undefined) {
      continue;
    }
    const contributor = contributingPhysicalEvent(physicalOperation);
    const identity = createVirtualOperationIdentity(virtualOperation);
    const existing = projections.get(identity.key);
    if (existing === undefined) {
      projections.set(
        identity.key,
        projectVirtualOperation(topology, virtualOperation, [contributor]),
      );
    } else {
      const operation =
        virtualOperation.precedence < existing.operation.precedence
          ? virtualOperation
          : existing.operation;
      projections.set(identity.key, {
        ...existing,
        operation,
        contributingPhysicalEvents: [
          ...existing.contributingPhysicalEvents,
          contributor,
        ],
      });
    }
  }
  return [...projections.values()];
}

export function selectDisplayedVirtualOperations(
  projections: readonly VirtualOperationProjection[],
): readonly VirtualOperationProjection[] {
  return projections.filter(
    (projection, index) =>
      !projections.some(
        (other, otherIndex) =>
          otherIndex !== index &&
          operationsOverlap(projection, other) &&
          (other.operation.precedence < projection.operation.precedence ||
            (other.operation.precedence === projection.operation.precedence &&
              otherIndex < index)),
      ),
  );
}

function operationsOverlap(
  first: VirtualOperationProjection,
  second: VirtualOperationProjection,
): boolean {
  return first.identity.normalizedTargetIds.some((target) =>
    second.identity.normalizedTargetIds.includes(target),
  );
}

function normalizedVirtualTargetIds(
  operation: ValidatedVirtualOperation,
): readonly number[] {
  if (operation.kind === "single") {
    return [operation.target];
  }
  if (operation.name === "CX") {
    return [operation.control, operation.target];
  }
  return operation.firstTarget < operation.secondTarget
    ? [operation.firstTarget, operation.secondTarget]
    : [operation.secondTarget, operation.firstTarget];
}

function singleVirtualGlyph(
  name: Extract<ValidatedVirtualOperation, { kind: "single" }>["name"],
): "T" | "H" | "S" | "⟨X❘" | "⟨Y❘" | "⟨Z❘" {
  switch (name) {
    case "T":
    case "H":
    case "S":
      return name;
    case "Mx":
      return "⟨X❘";
    case "My":
      return "⟨Y❘";
    case "Mz":
      return "⟨Z❘";
  }
}

function contributingPhysicalEvent(
  operation: ValidatedMajoranaOperation,
): ContributingPhysicalEvent {
  return {
    operation: operation.name,
    targetQubitIds:
      operation.kind === "single"
        ? [operation.target]
        : [operation.firstTarget, operation.secondTarget],
  };
}

function requireVirtualQubit(
  topology: VirtualTopology,
  virtualQubitId: number,
): void {
  if (!topology.qubitsById.has(virtualQubitId)) {
    throw new Error(
      `Validated virtual operation targets missing virtual qubit V${virtualQubitId}`,
    );
  }
}

function requireVirtualAdjacency(
  topology: VirtualTopology,
  operation: Extract<ValidatedVirtualOperation, { kind: "joint" }>,
  firstTarget: number,
  secondTarget: number,
): VirtualAdjacencyEdge {
  const edge = getVirtualAdjacencyEdge(topology, firstTarget, secondTarget);
  if (
    edge === undefined ||
    edge.id !== operation.adjacencyEdgeId ||
    edge.orientation !== operation.orientation ||
    edge.supportedOperation !== operation.name
  ) {
    throw new Error(
      `Validated virtual operation ${operation.name} has no matching adjacency edge`,
    );
  }
  return edge;
}
