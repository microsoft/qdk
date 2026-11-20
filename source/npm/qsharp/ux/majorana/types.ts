// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

export const MZM_IDS = [1, 2, 3, 4] as const;
export type MzmId = (typeof MZM_IDS)[number];
export type MzmPair = readonly [MzmId, MzmId];
export type Orientation = "horizontal" | "vertical";

export const SINGLE_OPERATIONS = {
  Mx: { overlay: "X", islandRoute: "none", mzmIds: [2, 4] },
  "My-up": { overlay: "Y", islandRoute: "upper", mzmIds: [1, 4] },
  "My-lw": { overlay: "Y", islandRoute: "lower", mzmIds: [1, 4] },
  "Mz-up": { overlay: "Z", islandRoute: "upper", mzmIds: [1, 2] },
  "Mz-lw": { overlay: "Z", islandRoute: "lower", mzmIds: [3, 4] },
  "T-up": { overlay: "T", islandRoute: "upper", mzmIds: [1, 2] },
  "T-lw": { overlay: "T", islandRoute: "lower", mzmIds: [3, 4] },
} as const satisfies Record<
  string,
  { overlay: Pauli | "T"; islandRoute: IslandRoute; mzmIds: MzmPair }
>;

export const JOINT_OPERATIONS = {
  Mzz: {
    orientation: "vertical",
    paulis: ["Z", "Z"],
    mzmIds: [
      [3, 4],
      [1, 2],
    ],
  },
  Mzy: {
    orientation: "vertical",
    paulis: ["Z", "Y"],
    mzmIds: [
      [3, 4],
      [2, 3],
    ],
  },
  Myy: {
    orientation: "vertical",
    paulis: ["Y", "Y"],
    mzmIds: [
      [1, 4],
      [1, 4],
    ],
  },
  Myz: {
    orientation: "vertical",
    paulis: ["Y", "Z"],
    mzmIds: [
      [1, 4],
      [1, 2],
    ],
  },
  Mxx: {
    orientation: "horizontal",
    paulis: ["X", "X"],
    mzmIds: [
      [2, 4],
      [1, 3],
    ],
  },
} as const satisfies Record<
  string,
  {
    orientation: Orientation;
    paulis: readonly [Pauli, Pauli];
    mzmIds: readonly [MzmPair, MzmPair];
  }
>;

export const MAJORANA_VIEWS = ["Tetrons", "Qubits"] as const;

export type SingleQubitOperationName = keyof typeof SINGLE_OPERATIONS;
export type JointQubitOperationName = keyof typeof JOINT_OPERATIONS;
export type MajoranaOperationName =
  | SingleQubitOperationName
  | JointQubitOperationName;
export type MajoranaView = (typeof MAJORANA_VIEWS)[number];
export type MajoranaTransitionPhase = "stable" | "out" | "in";
export type Pauli = "X" | "Y" | "Z";

export type MajoranaDeviceSize = readonly [
  tetronRows: number,
  tetronColumns: number,
];

export type SingleQubitOperation = readonly [
  physicalOperation: SingleQubitOperationName,
  physicalTargets: readonly [target: number],
];

export type JointQubitOperation = readonly [
  physicalOperation: JointQubitOperationName,
  physicalTargets: readonly [first: number, second: number],
];

export type MajoranaOperation = SingleQubitOperation | JointQubitOperation;
export type MajoranaTraceStep = readonly MajoranaOperation[];
export type MajoranaTrace = readonly MajoranaTraceStep[];
export type MajoranaInput = readonly [
  deviceSize: MajoranaDeviceSize,
  trace: MajoranaTrace,
];

export type MajoranaPresentationOptions = {
  showQubitLabels?: boolean;
  showMzmLabels?: boolean;
};

export type MajoranaOptions = MajoranaPresentationOptions & {
  initialView?: MajoranaView;
};

export type ResolvedMajoranaOptions = Required<MajoranaOptions>;

export type MajoranaController = {
  updateInput(input: MajoranaInput): void;
  updatePresentation(options: MajoranaPresentationOptions): void;
  setView(view: MajoranaView): void;
  setStep(step: number): void;
  dispose(): void;
};

export type MajoranaValidationErrorCode =
  | "invalid-input-shape"
  | "invalid-device-size"
  | "invalid-trace"
  | "invalid-step"
  | "unknown-operation"
  | "invalid-target-count"
  | "invalid-target"
  | "duplicate-target"
  | "invalid-adjacency"
  | "invalid-target-order"
  | "unavailable-island"
  | "invalid-options";

export type ValidatedMajoranaDevice = {
  tetronRows: number;
  tetronColumns: number;
  qubitCount: number;
};

export type IslandRoute = "upper" | "lower" | "none";

export type ValidatedSingleOperation = {
  kind: "single";
  name: SingleQubitOperationName;
  target: number;
  pauli: Pauli | undefined;
  islandRoute: IslandRoute;
  operationType: "measurement" | "pulse";
};

export type ValidatedJointOperation = {
  kind: "joint";
  name: JointQubitOperationName;
  firstTarget: number;
  secondTarget: number;
  orientation: "horizontal" | "vertical";
  firstPauli: Pauli;
  secondPauli: Pauli;
};

export type ValidatedMajoranaOperation =
  | ValidatedSingleOperation
  | ValidatedJointOperation;
export type ValidatedMajoranaTraceStep = readonly ValidatedMajoranaOperation[];

export type ValidatedMajoranaInput = {
  device: ValidatedMajoranaDevice;
  trace: readonly ValidatedMajoranaTraceStep[];
};
