// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { getTetronGridPosition } from "./topology.js";
import {
  JOINT_QUBIT_OPERATION_NAMES,
  MAJORANA_VIEWS,
  SINGLE_QUBIT_OPERATION_NAMES,
  type IslandRoute,
  type JointQubitOperationName,
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
} from "./types.js";

const SINGLE_OPERATION_NAMES = new Set<string>(SINGLE_QUBIT_OPERATION_NAMES);
const JOINT_OPERATION_NAMES = new Set<string>(JOINT_QUBIT_OPERATION_NAMES);
const VIEW_NAMES = new Set<string>(MAJORANA_VIEWS);
const OPTION_NAMES = new Set([
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
  "T-up": {
    pauli: undefined,
    islandRoute: "upper",
    operationType: "pulse",
  },
  "T-lw": {
    pauli: undefined,
    islandRoute: "lower",
    operationType: "pulse",
  },
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

export function validateMajoranaInput(input: unknown): ValidatedMajoranaInput {
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
      validateStep(trace[stepIndex], stepIndex, device),
    ),
  };
}

export function validateMajoranaOptions(
  options: unknown = {},
): ResolvedMajoranaOptions {
  const values = validateOptionsObject(options, OPTION_NAMES);
  const showQubitLabels = validateBooleanOption(
    values,
    "showQubitLabels",
    true,
  );
  const showMzmLabels = validateBooleanOption(values, "showMzmLabels", false);
  const initialView = values.initialView ?? "Tetrons";
  if (typeof initialView !== "string" || !VIEW_NAMES.has(initialView)) {
    fail(
      "invalid-options",
      "options.initialView",
      'expected "Tetrons" or "Qubits"',
      initialView,
    );
  }

  return {
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
      "expected [tetronColumns, tetronRows]",
      value,
    );
  }

  const [tetronRows, tetronColumns] = value;
  if (!isDimension(tetronColumns)) {
    fail(
      "invalid-device-size",
      "deviceSize[0]",
      "expected a positive integer",
      tetronColumns,
    );
  }
  if (!isDimension(tetronRows)) {
    fail(
      "invalid-device-size",
      "deviceSize[1]",
      "expected a positive integer",
      tetronRows,
    );
  }

  return {
    tetronRows,
    tetronColumns,
    qubitCount: tetronRows * tetronColumns,
  };
}

function validateStep(
  value: unknown,
  stepIndex: number,
  device: ValidatedMajoranaDevice,
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
    ),
  );
}

function validateOperation(
  value: unknown,
  path: string,
  device: ValidatedMajoranaDevice,
): ValidatedMajoranaOperation {
  if (!Array.isArray(value) || value.length !== 2) {
    fail(
      "invalid-step",
      path,
      "expected [physicalOperation, physicalTargets]",
      value,
    );
  }

  const [name, targets] = value;
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
    );
  }
  return validateJointOperation(
    name as JointQubitOperationName,
    targets,
    path,
    device,
  );
}

function validateSingleOperation(
  name: SingleQubitOperationName,
  targets: unknown,
  path: string,
  device: ValidatedMajoranaDevice,
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
  const position = getTetronGridPosition(target, device);
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
  };
}

function validateJointOperation(
  name: JointQubitOperationName,
  targets: unknown,
  path: string,
  device: ValidatedMajoranaDevice,
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
  const first = getTetronGridPosition(firstTarget, device);
  const second = getTetronGridPosition(secondTarget, device);
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
  };
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
  return typeof value === "number" && Number.isInteger(value) && value >= 1;
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
    // Describe the array by recursively describing its elements.
    return `[${value.map(describeValue).join(", ")}]`;
  }
  return typeof value;
}
