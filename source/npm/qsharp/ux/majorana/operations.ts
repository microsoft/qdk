// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import type {
  Pauli,
  SingleQubitOperationName,
  JointQubitOperationName,
  ValidatedMajoranaOperation,
  ValidatedMajoranaTraceStep,
} from "./types.js";
import {
  getAdjacencyEdge,
  getAdjacentIsland,
  type AdjacencyEdge,
  type LinearIsland,
  type MajoranaTopology,
  type MzmId,
} from "./topology.js";

export type OperationOverlay = {
  qubitId: number;
  value: "⟨X❘" | "⟨Y❘" | "⟨Z❘" | "T";
};

export type TetronMzmPair = {
  qubitId: number;
  mzmIds: readonly [MzmId, MzmId];
};

export type LoopRoute =
  | {
      kind: "single";
      tetron: TetronMzmPair;
      islandId: string | undefined;
    }
  | {
      kind: "joint";
      first: TetronMzmPair;
      second: TetronMzmPair;
      orientation: AdjacencyEdge["orientation"];
    };

export type OperationProjection = {
  operation: ValidatedMajoranaOperation;
  targetQubitIds: readonly number[];
  overlays: readonly OperationOverlay[];
  activeAdjacencyEdgeId: string | undefined;
  loopRoute: LoopRoute;
  stroke: "solid" | "dotted";
};

const SINGLE_MZM_PAIRS: Record<
  SingleQubitOperationName,
  readonly [MzmId, MzmId]
> = {
  Mx: [2, 4],
  "My-up": [1, 4],
  "My-lw": [1, 4],
  "Mz-up": [1, 2],
  "Mz-lw": [3, 4],
  T: [2, 4],
};

const JOINT_MZM_PAIRS: Record<
  JointQubitOperationName,
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
    const island = resolveIsland(topology, operation);
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
      activeAdjacencyEdgeId: undefined,
      loopRoute: {
        kind: "single",
        tetron: {
          qubitId: operation.target,
          mzmIds: SINGLE_MZM_PAIRS[operation.name],
        },
        islandId: island?.id,
      },
      stroke: operation.operationType === "pulse" ? "dotted" : "solid",
    };
  }

  const pairs = JOINT_MZM_PAIRS[operation.name];
  const edge = resolveAdjacency(topology, operation);
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
    activeAdjacencyEdgeId: edge.id,
    loopRoute: {
      kind: "joint",
      first: { qubitId: operation.firstTarget, mzmIds: pairs[0] },
      second: { qubitId: operation.secondTarget, mzmIds: pairs[1] },
      orientation: edge.orientation,
    },
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
