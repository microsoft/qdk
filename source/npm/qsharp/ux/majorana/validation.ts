// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import {
  JOINT_QUBIT_OPERATION_NAMES,
  MAJORANA_VIEWS,
  SINGLE_QUBIT_OPERATION_NAMES,
  VIRTUAL_OPERATION_NAMES,
  type IslandRoute,
  type JointQubitOperationName,
  type MajoranaInputValidationOptions,
  type MajoranaPresentationOptions,
  type MajoranaValidationErrorCode,
  type Pauli,
  type ResolvedMajoranaOptions,
  type SingleQubitOperationName,
  type ValidatedJointOperation,
  type ValidatedMajoranaDevice,
  type ValidatedMajoranaInput,
  type ValidatedMajoranaOperation,
  type ValidatedSingleOperation,
  type ValidatedSingleVirtualOperation,
  type ValidatedVirtualOperation,
} from "./types.js";
import {
  getVirtualAdjacency,
  getVirtualQubitCount,
} from "./virtual-topology.js";

const SINGLE_OPERATION_NAMES = new Set<string>(SINGLE_QUBIT_OPERATION_NAMES);
const JOINT_OPERATION_NAMES = new Set<string>(JOINT_QUBIT_OPERATION_NAMES);
const VIEW_NAMES = new Set<string>(MAJORANA_VIEWS);
const VIRTUAL_OPERATION_NAME_SET = new Set<string>(VIRTUAL_OPERATION_NAMES);
const SINGLE_VIRTUAL_OPERATION_NAMES = new Set<string>([
  "T",
  "H",
  "S",
  "Mx",
  "My",
  "Mz",
]);
const OPTION_NAMES = new Set([
  "enableVirtualView",
  "showQubitLabels",
  "showMzmLabels",
  "initialView",
]);
const PRESENTATION_OPTION_NAMES = new Set(["showQubitLabels", "showMzmLabels"]);

export class MajoranaValidationError extends Error {
  constructor(
    public readonly code: MajoranaValidationErrorCode,
    public readonly path: string,
    message: string,
  ) {
    super(`${path}: ${message}`);
    this.name = "MajoranaValidationError";
  }
}

type TetronPosition = {
  row: number;
  column: number;
};

type SingleOperationMetadata = {
  pauli: Pauli | undefined;
  islandRoute: IslandRoute;
  operationType: "measurement" | "pulse";
};

const SINGLE_OPERATION_METADATA: Record<
  SingleQubitOperationName,
  SingleOperationMetadata
> = {
  Mx: { pauli: "X", islandRoute: "none", operationType: "measurement" },
  "My-up": {
    pauli: "Y",
    islandRoute: "upper",
    operationType: "measurement",
  },
  "My-lw": {
    pauli: "Y",
    islandRoute: "lower",
    operationType: "measurement",
  },
  "Mz-up": {
    pauli: "Z",
    islandRoute: "upper",
    operationType: "measurement",
  },
  "Mz-lw": {
    pauli: "Z",
    islandRoute: "lower",
    operationType: "measurement",
  },
  T: { pauli: undefined, islandRoute: "none", operationType: "pulse" },
};

const JOINT_OPERATION_METADATA: Record<
  JointQubitOperationName,
  {
    orientation: "horizontal" | "vertical";
    firstPauli: Pauli;
    secondPauli: Pauli;
  }
> = {
  Mzz: { orientation: "vertical", firstPauli: "Z", secondPauli: "Z" },
  Mzy: { orientation: "vertical", firstPauli: "Z", secondPauli: "Y" },
  Myy: { orientation: "vertical", firstPauli: "Y", secondPauli: "Y" },
  Myz: { orientation: "vertical", firstPauli: "Y", secondPauli: "Z" },
  Mxx: { orientation: "horizontal", firstPauli: "X", secondPauli: "X" },
};

export function validateMajoranaInput(
  input: unknown,
  options: MajoranaInputValidationOptions = {},
): ValidatedMajoranaInput {
  const enableVirtualView = validateInputValidationOptions(options);
  if (!Array.isArray(input) || input.length !== 2) {
    fail("invalid-input-shape", "input", "expected [deviceSize, trace]", input);
  }

  const device = validateDeviceSize(input[0]);
  const trace = input[1];
  if (!Array.isArray(trace) || trace.length === 0) {
    fail("invalid-trace", "trace", "expected a non-empty array", trace);
  }

  return {
    device,
    trace: Array.from({ length: trace.length }, (_, stepIndex) =>
      validateStep(trace[stepIndex], stepIndex, device, enableVirtualView),
    ),
  };
}

export function validateMajoranaOptions(
  options: unknown = {},
): ResolvedMajoranaOptions {
  const values = validateOptionsObject(options, OPTION_NAMES);
  const enableVirtualView = validateBooleanOption(
    values,
    "enableVirtualView",
    false,
  );
  const showQubitLabels = validateBooleanOption(
    values,
    "showQubitLabels",
    true,
  );
  const showMzmLabels = validateBooleanOption(values, "showMzmLabels", false);
  const initialView =
    values.initialView ?? (enableVirtualView ? "Virtual" : "Tetrons");
  if (typeof initialView !== "string" || !VIEW_NAMES.has(initialView)) {
    fail(
      "invalid-options",
      "options.initialView",
      'expected "Tetrons", "Qubits", or "Virtual"',
      initialView,
    );
  }
  if (initialView === "Virtual" && !enableVirtualView) {
    fail(
      "invalid-options",
      "options.initialView",
      'expected "Tetrons" or "Qubits" when the virtual view is disabled',
      initialView,
    );
  }

  return {
    enableVirtualView,
    showQubitLabels,
    showMzmLabels,
    initialView: initialView as ResolvedMajoranaOptions["initialView"],
  };
}

export function validateMajoranaPresentationOptions(
  options: unknown,
): MajoranaPresentationOptions {
  const values = validateOptionsObject(options, PRESENTATION_OPTION_NAMES);
  const result: MajoranaPresentationOptions = {};
  if ("showQubitLabels" in values) {
    result.showQubitLabels = validateBooleanOption(
      values,
      "showQubitLabels",
      true,
    );
  }
  if ("showMzmLabels" in values) {
    result.showMzmLabels = validateBooleanOption(
      values,
      "showMzmLabels",
      false,
    );
  }
  return result;
}

function validateDeviceSize(value: unknown): ValidatedMajoranaDevice {
  if (!Array.isArray(value) || value.length !== 2) {
    fail(
      "invalid-device-size",
      "deviceSize",
      "expected [cellRows, cellColumns]",
      value,
    );
  }

  const [cellRows, cellColumns] = value;
  if (!isDimension(cellRows)) {
    fail(
      "invalid-device-size",
      "deviceSize[0]",
      "expected an integer from 1 through 4",
      cellRows,
    );
  }
  if (!isDimension(cellColumns)) {
    fail(
      "invalid-device-size",
      "deviceSize[1]",
      "expected an integer from 1 through 4",
      cellColumns,
    );
  }

  return {
    cellRows,
    cellColumns,
    tetronRows: cellRows * 2,
    tetronColumns: cellColumns * 2,
    qubitCount: cellRows * cellColumns * 4,
  };
}

function validateStep(
  value: unknown,
  stepIndex: number,
  device: ValidatedMajoranaDevice,
  enableVirtualView: boolean,
): readonly ValidatedMajoranaOperation[] {
  const path = `trace[${stepIndex}]`;
  if (!Array.isArray(value) || value.length === 0) {
    fail("invalid-step", path, "expected a non-empty array", value);
  }

  return Array.from({ length: value.length }, (_, operationIndex) =>
    validateOperation(
      value[operationIndex],
      `${path}[${operationIndex}]`,
      device,
      enableVirtualView,
    ),
  );
}

function validateOperation(
  value: unknown,
  path: string,
  device: ValidatedMajoranaDevice,
  enableVirtualView: boolean,
): ValidatedMajoranaOperation {
  if (!Array.isArray(value) || value.length !== 3) {
    fail(
      "invalid-step",
      path,
      "expected [physicalOperation, physicalTargets, virtualOperation]",
      value,
    );
  }

  const [name, targets, virtualMetadata] = value;
  if (
    typeof name !== "string" ||
    (!SINGLE_OPERATION_NAMES.has(name) && !JOINT_OPERATION_NAMES.has(name))
  ) {
    fail(
      "unknown-operation",
      `${path}.operation`,
      "expected a supported operation name",
      name,
    );
  }

  if (SINGLE_OPERATION_NAMES.has(name)) {
    return validateSingleOperation(
      name as SingleQubitOperationName,
      targets,
      path,
      device,
      virtualMetadata,
      enableVirtualView,
    );
  }
  return validateJointOperation(
    name as JointQubitOperationName,
    targets,
    path,
    device,
    virtualMetadata,
    enableVirtualView,
  );
}

function validateSingleOperation(
  name: SingleQubitOperationName,
  targets: unknown,
  path: string,
  device: ValidatedMajoranaDevice,
  virtualMetadata: unknown,
  enableVirtualView: boolean,
): ValidatedSingleOperation {
  if (!Array.isArray(targets) || targets.length !== 1) {
    fail(
      "invalid-target-count",
      `${path}.targets`,
      "expected exactly one target",
      targets,
    );
  }

  const target = validateTarget(targets[0], `${path}.targets[0]`, device);
  const metadata = SINGLE_OPERATION_METADATA[name];
  const position = getTetronPosition(target, device.cellRows);
  if (metadata.islandRoute === "upper" && position.row === 0) {
    fail(
      "unavailable-island",
      `${path}.targets[0]`,
      `${name} requires an island above the target`,
      target,
    );
  }
  if (
    metadata.islandRoute === "lower" &&
    position.row === device.tetronRows - 1
  ) {
    fail(
      "unavailable-island",
      `${path}.targets[0]`,
      `${name} requires an island below the target`,
      target,
    );
  }

  return {
    kind: "single",
    name,
    target,
    ...metadata,
    virtualOperation: validateVirtualOperation(
      virtualMetadata,
      `${path}.virtualOperation`,
      device,
      enableVirtualView,
    ),
  };
}

function validateJointOperation(
  name: JointQubitOperationName,
  targets: unknown,
  path: string,
  device: ValidatedMajoranaDevice,
  virtualMetadata: unknown,
  enableVirtualView: boolean,
): ValidatedJointOperation {
  if (!Array.isArray(targets) || targets.length !== 2) {
    fail(
      "invalid-target-count",
      `${path}.targets`,
      "expected exactly two targets",
      targets,
    );
  }

  const firstTarget = validateTarget(targets[0], `${path}.targets[0]`, device);
  const secondTarget = validateTarget(targets[1], `${path}.targets[1]`, device);
  if (firstTarget === secondTarget) {
    fail(
      "duplicate-target",
      `${path}.targets`,
      "expected two distinct targets",
      targets,
    );
  }

  const metadata = JOINT_OPERATION_METADATA[name];
  const first = getTetronPosition(firstTarget, device.cellRows);
  const second = getTetronPosition(secondTarget, device.cellRows);
  if (metadata.orientation === "horizontal") {
    if (
      first.row !== second.row ||
      Math.abs(first.column - second.column) !== 1
    ) {
      fail(
        "invalid-adjacency",
        `${path}.targets`,
        "expected horizontally adjacent targets",
        targets,
      );
    }
    if (first.column >= second.column) {
      fail(
        "invalid-target-order",
        `${path}.targets`,
        "expected targets ordered left then right",
        targets,
      );
    }
  } else {
    if (
      first.column !== second.column ||
      Math.abs(first.row - second.row) !== 1
    ) {
      fail(
        "invalid-adjacency",
        `${path}.targets`,
        "expected vertically adjacent targets",
        targets,
      );
    }
    if (first.row >= second.row) {
      fail(
        "invalid-target-order",
        `${path}.targets`,
        "expected targets ordered upper then lower",
        targets,
      );
    }
  }

  return {
    kind: "joint",
    name,
    firstTarget,
    secondTarget,
    ...metadata,
    virtualOperation: validateVirtualOperation(
      virtualMetadata,
      `${path}.virtualOperation`,
      device,
      enableVirtualView,
    ),
  };
}

function validateVirtualOperation(
  value: unknown,
  path: string,
  device: ValidatedMajoranaDevice,
  enableVirtualView: boolean,
): ValidatedVirtualOperation | undefined {
  if (!enableVirtualView) {
    return undefined;
  }
  if (!Array.isArray(value) || value.length !== 3) {
    fail(
      "invalid-virtual-metadata",
      path,
      "expected [operationName, targets, precedence]",
      value,
    );
  }

  const [name, targets, precedenceValue] = value;
  if (typeof name !== "string" || !VIRTUAL_OPERATION_NAME_SET.has(name)) {
    fail(
      "unknown-virtual-operation",
      `${path}.operation`,
      "expected a supported virtual operation name",
      name,
    );
  }

  const targetCount = SINGLE_VIRTUAL_OPERATION_NAMES.has(name) ? 1 : 2;
  if (!Array.isArray(targets) || targets.length !== targetCount) {
    fail(
      "invalid-virtual-target-count",
      `${path}.targets`,
      `expected exactly ${targetCount === 1 ? "one target" : "two targets"}`,
      targets,
    );
  }
  const normalizedTargets = targets.map((target, index) =>
    validateVirtualTarget(target, `${path}.targets[${index}]`, device),
  );
  const precedence = validateVirtualPrecedence(
    precedenceValue,
    `${path}.precedence`,
  );

  if (targetCount === 1) {
    return {
      kind: "single",
      name: name as ValidatedSingleVirtualOperation["name"],
      target: normalizedTargets[0],
      precedence,
    };
  }

  const [firstTarget, secondTarget] = normalizedTargets;
  if (firstTarget === secondTarget) {
    fail(
      "duplicate-virtual-target",
      `${path}.targets`,
      "expected two distinct virtual targets",
      targets,
    );
  }
  const adjacency = getVirtualAdjacency(device, firstTarget, secondTarget);
  if (adjacency === undefined || adjacency.supportedOperation !== name) {
    fail(
      "invalid-virtual-adjacency",
      `${path}.targets`,
      name === "CX"
        ? "expected horizontally adjacent virtual targets"
        : "expected vertically adjacent virtual targets",
      targets,
    );
  }
  if (name === "CX") {
    return {
      kind: "joint",
      name,
      control: firstTarget,
      target: secondTarget,
      precedence,
      orientation: "horizontal",
      adjacencyEdgeId: adjacency.id,
    };
  }
  return {
    kind: "joint",
    name: "CZ",
    firstTarget,
    secondTarget,
    precedence,
    orientation: "vertical",
    adjacencyEdgeId: adjacency.id,
  };
}

function validateVirtualPrecedence(value: unknown, path: string): number {
  if (typeof value !== "number" || !Number.isInteger(value) || value < 0) {
    fail(
      "invalid-virtual-precedence",
      path,
      "expected a non-negative integer",
      value,
    );
  }
  return value;
}

function validateVirtualTarget(
  value: unknown,
  path: string,
  device: ValidatedMajoranaDevice,
): number {
  const virtualQubitCount = getVirtualQubitCount(device);
  if (
    typeof value !== "number" ||
    !Number.isInteger(value) ||
    value < 0 ||
    value >= virtualQubitCount
  ) {
    fail(
      "invalid-virtual-target",
      path,
      `expected an integer from 0 through ${virtualQubitCount - 1}`,
      value,
    );
  }
  return value;
}

function validateTarget(
  value: unknown,
  path: string,
  device: ValidatedMajoranaDevice,
): number {
  if (
    typeof value !== "number" ||
    !Number.isInteger(value) ||
    value < 0 ||
    value >= device.qubitCount
  ) {
    fail(
      "invalid-target",
      path,
      `expected an integer from 0 through ${device.qubitCount - 1}`,
      value,
    );
  }
  return value;
}

function getTetronPosition(qubitId: number, cellRows: number): TetronPosition {
  const cellBlock = Math.floor(qubitId / 4);
  const cellColumn = Math.floor(cellBlock / cellRows);
  const cellRow = cellBlock % cellRows;
  const localNumber = qubitId % 4;
  return {
    row: cellRow * 2 + Math.floor(localNumber / 2),
    column: cellColumn * 2 + (localNumber % 2),
  };
}

function validateOptionsObject(
  options: unknown,
  allowedNames: ReadonlySet<string>,
): Record<string, unknown> {
  if (
    typeof options !== "object" ||
    options === null ||
    Array.isArray(options)
  ) {
    fail("invalid-options", "options", "expected an options object", options);
  }

  const values = options as Record<string, unknown>;
  const unknownName = Object.keys(values).find(
    (name) => !allowedNames.has(name),
  );
  if (unknownName !== undefined) {
    fail(
      "invalid-options",
      `options.${unknownName}`,
      "unknown presentation option",
      unknownName,
    );
  }
  return values;
}

function validateInputValidationOptions(
  options: MajoranaInputValidationOptions,
): boolean {
  const values = validateOptionsObject(options, new Set(["enableVirtualView"]));
  return validateBooleanOption(values, "enableVirtualView", false);
}

function validateBooleanOption(
  options: Record<string, unknown>,
  name: string,
  defaultValue: boolean,
): boolean {
  const value = options[name];
  if (value === undefined) {
    return defaultValue;
  }
  if (typeof value !== "boolean") {
    fail("invalid-options", `options.${name}`, "expected a boolean", value);
  }
  return value;
}

function isDimension(value: unknown): value is number {
  return (
    typeof value === "number" &&
    Number.isInteger(value) &&
    value >= 1 &&
    value <= 4
  );
}

function fail(
  code: MajoranaValidationErrorCode,
  path: string,
  message: string,
  received: unknown,
): never {
  throw new MajoranaValidationError(
    code,
    path,
    `${message}; received ${describeValue(received)}`,
  );
}

function describeValue(value: unknown): string {
  if (typeof value === "string") {
    return JSON.stringify(value);
  }
  if (
    value === null ||
    value === undefined ||
    typeof value === "number" ||
    typeof value === "boolean" ||
    typeof value === "bigint"
  ) {
    return String(value);
  }
  if (Array.isArray(value)) {
    return `Array(length=${value.length})`;
  }
  return typeof value;
}
