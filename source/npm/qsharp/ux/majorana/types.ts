// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

export const SINGLE_QUBIT_OPERATION_NAMES = [
  "Mx",
  "My-up",
  "My-lw",
  "Mz-up",
  "Mz-lw",
  "T",
] as const;

export const JOINT_QUBIT_OPERATION_NAMES = [
  "Mzz",
  "Mzy",
  "Myy",
  "Myz",
  "Mxx",
] as const;

export const VIRTUAL_OPERATION_NAMES = [
  "T",
  "H",
  "S",
  "Mx",
  "My",
  "Mz",
  "CX",
  "CZ",
] as const;

export const MAJORANA_VIEWS = ["Tetrons", "Qubits", "Virtual"] as const;

export type SingleQubitOperationName =
  (typeof SINGLE_QUBIT_OPERATION_NAMES)[number];
export type JointQubitOperationName =
  (typeof JOINT_QUBIT_OPERATION_NAMES)[number];
export type MajoranaOperationName =
  | SingleQubitOperationName
  | JointQubitOperationName;
export type VirtualOperationName = (typeof VIRTUAL_OPERATION_NAMES)[number];
export type MajoranaView = (typeof MAJORANA_VIEWS)[number];
export type MajoranaTransitionPhase = "stable" | "out" | "in";
export type Pauli = "X" | "Y" | "Z";

export type MajoranaDeviceSize = readonly [
  cellRows: number,
  cellColumns: number,
];

export type SingleVirtualOperationTraceEntry = readonly [
  operation: "T" | "H" | "S" | "Mx" | "My" | "Mz",
  targets: readonly [target: number],
  precedence: number,
];

export type JointVirtualOperationTraceEntry = readonly [
  operation: "CX" | "CZ",
  targets: readonly [first: number, second: number],
  precedence: number,
];

export type VirtualOperationTraceEntry =
  | SingleVirtualOperationTraceEntry
  | JointVirtualOperationTraceEntry;

export type SingleQubitOperation = readonly [
  physicalOperation: SingleQubitOperationName,
  physicalTargets: readonly [target: number],
  virtualOperation: VirtualOperationTraceEntry | null,
];

export type JointQubitOperation = readonly [
  physicalOperation: JointQubitOperationName,
  physicalTargets: readonly [first: number, second: number],
  virtualOperation: VirtualOperationTraceEntry | null,
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
  enableVirtualView?: boolean;
  initialView?: MajoranaView;
};

export type ResolvedMajoranaOptions = {
  enableVirtualView: boolean;
  showQubitLabels: boolean;
  showMzmLabels: boolean;
  initialView: MajoranaView;
};

export type MajoranaInputValidationOptions = {
  enableVirtualView?: boolean;
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
  | "invalid-virtual-metadata"
  | "unknown-virtual-operation"
  | "invalid-virtual-target-count"
  | "invalid-virtual-target"
  | "invalid-virtual-precedence"
  | "duplicate-virtual-target"
  | "invalid-virtual-adjacency"
  | "invalid-options";

export type ValidatedMajoranaDevice = {
  cellRows: number;
  cellColumns: number;
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
  virtualOperation: ValidatedVirtualOperation | undefined;
};

export type ValidatedJointOperation = {
  kind: "joint";
  name: JointQubitOperationName;
  firstTarget: number;
  secondTarget: number;
  orientation: "horizontal" | "vertical";
  firstPauli: Pauli;
  secondPauli: Pauli;
  virtualOperation: ValidatedVirtualOperation | undefined;
};

export type ValidatedSingleVirtualOperation = {
  kind: "single";
  name: "T" | "H" | "S" | "Mx" | "My" | "Mz";
  target: number;
  precedence: number;
};

export type ValidatedCxOperation = {
  kind: "joint";
  name: "CX";
  control: number;
  target: number;
  precedence: number;
  orientation: "horizontal";
  adjacencyEdgeId: string;
};

export type ValidatedCzOperation = {
  kind: "joint";
  name: "CZ";
  firstTarget: number;
  secondTarget: number;
  precedence: number;
  orientation: "vertical";
  adjacencyEdgeId: string;
};

export type ValidatedVirtualOperation =
  | ValidatedSingleVirtualOperation
  | ValidatedCxOperation
  | ValidatedCzOperation;

export type ValidatedMajoranaOperation =
  | ValidatedSingleOperation
  | ValidatedJointOperation;
export type ValidatedMajoranaTraceStep = readonly ValidatedMajoranaOperation[];

export type ValidatedMajoranaInput = {
  device: ValidatedMajoranaDevice;
  trace: readonly ValidatedMajoranaTraceStep[];
};
