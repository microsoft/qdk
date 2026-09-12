// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import type {
  Pauli,
  ValidatedMajoranaOperation,
  ValidatedMajoranaTraceStep,
} from "./types.js";
import {
  getAdjacencyEdge,
  getAdjacentIsland,
  getMzmNodeId,
  type AdjacencyEdge,
  type LinearIsland,
  type MajoranaTopology,
  type MzmId,
} from "./topology.js";

export type OperationOverlay = {
  qubitId: number;
  value: "⟨X❘" | "⟨Y❘" | "⟨Z❘" | "T";
};

export type LoopNode =
  | {
      kind: "mzm";
      id: string;
      qubitId: number;
      mzmId: MzmId;
    }
  | {
      kind: "island";
      id: string;
    };

export type LoopRoute = {
  closed: true;
  nodes: readonly LoopNode[];
};

export type OperationProjection = {
  operation: ValidatedMajoranaOperation;
  targetQubitIds: readonly number[];
  overlays: readonly OperationOverlay[];
  participatingMzmNodeIds: readonly string[];
  participatingIslandId: string | undefined;
  activeAdjacencyEdgeId: string | undefined;
  loopRoute: LoopRoute;
  stroke: "solid" | "dotted";
};

const SINGLE_MZM_PAIRS: Record<
  ValidatedMajoranaOperation["name"],
  readonly [MzmId, MzmId] | undefined
> = {
  Mx: [2, 4],
  "My-up": [1, 4],
  "My-lw": [1, 4],
  "Mz-up": [1, 2],
  "Mz-lw": [3, 4],
  T: [2, 4],
  Mzz: undefined,
  Mzy: undefined,
  Myy: undefined,
  Myz: undefined,
  Mxx: undefined,
};

const JOINT_MZM_PAIRS: Record<
  "Mzz" | "Mzy" | "Myy" | "Myz" | "Mxx",
  readonly [first: readonly [MzmId, MzmId], second: readonly [MzmId, MzmId]]
> = {
  Mzz: [
    [3, 4],
    [1, 2],
  ],
  Mzy: [
    [3, 4],
    [2, 3],
  ],
  Myy: [
    [1, 4],
    [1, 4],
  ],
  Myz: [
    [1, 4],
    [1, 2],
  ],
  Mxx: [
    [2, 4],
    [1, 3],
  ],
};

export function projectOperation(
  topology: MajoranaTopology,
  operation: ValidatedMajoranaOperation,
): OperationProjection {
  if (operation.kind === "single") {
    const pair = SINGLE_MZM_PAIRS[operation.name];
    if (pair === undefined) {
      throw new Error(`Missing MZM mapping for ${operation.name}`);
    }
    const mzmNodes = pair.map((mzmId) =>
      createMzmLoopNode(operation.target, mzmId),
    );
    const island = resolveIsland(topology, operation);
    const nodes =
      island === undefined
        ? mzmNodes
        : [mzmNodes[0], createIslandLoopNode(island), mzmNodes[1]];
    return {
      operation,
      targetQubitIds: [operation.target],
      overlays: [
        {
          qubitId: operation.target,
          value:
            operation.operationType === "pulse"
              ? "T"
              : pauliOverlay(operation.pauli),
        },
      ],
      participatingMzmNodeIds: mzmNodes.map((node) => node.id),
      participatingIslandId: island?.id,
      activeAdjacencyEdgeId: undefined,
      loopRoute: { closed: true, nodes },
      stroke: operation.operationType === "pulse" ? "dotted" : "solid",
    };
  }

  const pairs = JOINT_MZM_PAIRS[operation.name];
  const firstNodes = pairs[0].map((mzmId) =>
    createMzmLoopNode(operation.firstTarget, mzmId),
  );
  const secondNodes = pairs[1].map((mzmId) =>
    createMzmLoopNode(operation.secondTarget, mzmId),
  );
  const edge = resolveAdjacency(topology, operation);
  const loopNodes = [...firstNodes, ...secondNodes];
  return {
    operation,
    targetQubitIds: [operation.firstTarget, operation.secondTarget],
    overlays: [
      {
        qubitId: operation.firstTarget,
        value: pauliOverlay(operation.firstPauli),
      },
      {
        qubitId: operation.secondTarget,
        value: pauliOverlay(operation.secondPauli),
      },
    ],
    participatingMzmNodeIds: loopNodes.map((node) => node.id),
    participatingIslandId: undefined,
    activeAdjacencyEdgeId: edge.id,
    loopRoute: { closed: true, nodes: loopNodes },
    stroke: "solid",
  };
}

export function projectTraceStep(
  topology: MajoranaTopology,
  step: ValidatedMajoranaTraceStep,
): readonly OperationProjection[] {
  return step.map((operation) => projectOperation(topology, operation));
}

function resolveIsland(
  topology: MajoranaTopology,
  operation: Extract<ValidatedMajoranaOperation, { kind: "single" }>,
): LinearIsland | undefined {
  if (operation.islandRoute === "none") {
    return undefined;
  }
  const island = getAdjacentIsland(
    topology,
    operation.target,
    operation.islandRoute,
  );
  if (island === undefined) {
    throw new Error(
      `Validated operation ${operation.name} has no ${operation.islandRoute} island`,
    );
  }
  return island;
}

function resolveAdjacency(
  topology: MajoranaTopology,
  operation: Extract<ValidatedMajoranaOperation, { kind: "joint" }>,
): AdjacencyEdge {
  const edge = getAdjacencyEdge(
    topology,
    operation.orientation,
    operation.firstTarget,
    operation.secondTarget,
  );
  if (edge === undefined) {
    throw new Error(
      `Validated operation ${operation.name} has no matching adjacency edge`,
    );
  }
  return edge;
}

function createMzmLoopNode(qubitId: number, mzmId: MzmId): LoopNode {
  return {
    kind: "mzm",
    id: getMzmNodeId(qubitId, mzmId),
    qubitId,
    mzmId,
  };
}

function createIslandLoopNode(island: LinearIsland): LoopNode {
  return {
    kind: "island",
    id: island.id,
  };
}

function pauliOverlay(pauli: Pauli | undefined): OperationOverlay["value"] {
  switch (pauli) {
    case "X":
      return "⟨X❘";
    case "Y":
      return "⟨Y❘";
    case "Z":
      return "⟨Z❘";
    case undefined:
      throw new Error("Measurement operation is missing its Pauli operator");
  }
}
