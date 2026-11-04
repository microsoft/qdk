// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

export const SINGLE_QUBIT_OPERATION_NAMES = [
  "Mx",
  "My-up",
  "My-lw",
  "Mz-up",
  "Mz-lw",
  "T-up",
  "T-lw",
] as const;

export const JOINT_QUBIT_OPERATION_NAMES = [
  "Mzz",
  "Mzy",
  "Myy",
  "Myz",
  "Mxx",
] as const;

export const MAJORANA_VIEWS = ["Tetrons", "Qubits"] as const;

export type SingleQubitOperationName =
  (typeof SINGLE_QUBIT_OPERATION_NAMES)[number];
export type JointQubitOperationName =
  (typeof JOINT_QUBIT_OPERATION_NAMES)[number];
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

export type ResolvedMajoranaOptions = {
  showQubitLabels: boolean;
  showMzmLabels: boolean;
  initialView: MajoranaView;
};

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
