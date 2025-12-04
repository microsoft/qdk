// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { isRegister, Register } from "./register.js";

/**
 * Current format version.
 */
export const CURRENT_VERSION = 1;

export interface CircuitGroup {
  circuits: Circuit[];
  version: number;
}

/**
 * Runtime check: is this a valid CircuitGroup?
 */
export function isCircuitGroup(obj: any): obj is CircuitGroup {
  return (
    obj &&
    typeof obj === "object" &&
    typeof obj.version === "number" &&
    Array.isArray(obj.circuits) &&
    obj.circuits.length > 0 &&
    obj.circuits.every(isCircuit)
  );
}

/**
 * Circuit to be visualized.
 */
export interface Circuit {
  /** Array of qubit resources. */
  qubits: Qubit[];
  componentGrid: ComponentGrid;
}

/**
 * Runtime check: is this a valid Circuit?
 */
export function isCircuit(obj: any): obj is Circuit {
  return (
    obj &&
    typeof obj === "object" &&
    Array.isArray(obj.qubits) &&
    obj.qubits.every(isQubit) &&
    Array.isArray(obj.componentGrid) &&
    obj.componentGrid.every(isColumn)
  );
}

export type ComponentGrid = Column[];

export interface Column {
  components: Component[];
}

/**
 * Runtime check: is this a valid Column?
 */
export function isColumn(obj: any): obj is Column {
  return (
    obj &&
    typeof obj === "object" &&
    Array.isArray(obj.components) &&
    obj.components.every(isOperation)
  );
}

/**
 * Represents a component of a circuit. Currently, the only component is an operation.
 * In the future, this may be extended to include other components.
 */
export type Component = Operation;

/**
 * Represents a unique qubit resource bit.
 */
export interface Qubit {
  /** Qubit ID. */
  id: number;
  /** Number of measurement results associated to the qubit. */
  numResults?: number;
  /** Declaration locations in the source code. */
  declarations?: SourceLocation[];
}

/**
 * Runtime check: is this a valid Qubit?
 */
export function isQubit(obj: any): obj is Qubit {
  return (
    obj &&
    typeof obj === "object" &&
    typeof obj.id === "number" &&
    // numResults is optional, but if present must be a number
    (obj.numResults === undefined || typeof obj.numResults === "number")
  );
}

/**
 * Base type for operations.
 */
export interface BaseOperation {
  /** Gate label. */
  gate: string;
  /** Formatted gate arguments. */
  args?: string[];
  /** The parameters expected for the operation. */
  params?: Parameter[];
  /** Nested operations within this operation. */
  children?: ComponentGrid;

  /** Custom data attributes to attach to gate element.
  Note that this is never written to file, so it is not part of the circuit schema */
  dataAttributes?: DataAttributes;

  /** Whether gate is a conditional operation. */
  isConditional?: boolean;
  /** Specify conditions on when to render operation. */
  conditionalRender?: ConditionalRender;

  /** Not written to file */
  metadata?: Metadata;
}

/**
 * Runtime check: is this a valid BaseOperation?
 */
function isBaseOperation(obj: any): obj is BaseOperation {
  return (
    obj &&
    typeof obj === "object" &&
    typeof obj.gate === "string" &&
    // args is optional, but if present must be an array of strings
    (obj.args === undefined ||
      (Array.isArray(obj.args) &&
        obj.args.every((arg: any) => typeof arg === "string"))) &&
    // params is optional, but if present must be an array of Parameter
    (obj.params === undefined ||
      (Array.isArray(obj.params) && obj.params.every(isParameter))) &&
    // children is optional, but if present must be a ComponentGrid
    (obj.children === undefined ||
      (Array.isArray(obj.children) && obj.children.every(isColumn))) &&
    // dataAttributes is optional, but if present must be an object with string values
    (obj.dataAttributes === undefined ||
      (typeof obj.dataAttributes === "object" &&
        obj.dataAttributes !== null &&
        Object.values(obj.dataAttributes).every(
          (val) => typeof val === "string",
        ))) &&
    // isConditional is optional, but if present must be boolean
    (obj.isConditional === undefined ||
      typeof obj.isConditional === "boolean") &&
    // conditionalRender is optional, but if present must be a valid enum value
    (obj.conditionalRender === undefined ||
      Object.values(ConditionalRender).includes(obj.conditionalRender))
  );
}

/**
 * Represents a measurement operation and the registers it acts on.
 */
export interface Measurement extends BaseOperation {
  /** Discriminator for the Operation type */
  kind: "measurement";
  /** The qubit registers the gate measures. */
  qubits: Register[];
  /** The classical registers the gate writes to. */
  results: Register[];
}

/**
 * Represents a unitary operation and the registers it acts on.
 */
export interface Unitary extends BaseOperation {
  /** Discriminator for the Operation type */
  kind: "unitary";
  /** Target registers the gate acts on. */
  targets: Register[];
  /** Control registers the gate acts on. */
  controls?: Register[];
  /** Whether gate is an adjoint operation. */
  isAdjoint?: boolean;
}

/**
 * Represents a gate that sets its targets to a specific state.
 */
export interface Ket extends BaseOperation {
  /** Discriminator for the Operation type */
  kind: "ket";
  /** Target registers the gate acts on. */
  targets: Register[];
}

/**
 * Union type for operations.
 */
export type Operation = Unitary | Measurement | Ket;

/**
 * Runtime check: is this a valid Operation?
 */
export function isOperation(obj: any): obj is Operation {
  if (!isBaseOperation(obj)) return false;
  // Re-cast to any so we can check discriminated fields without narrowing
  const op: any = obj;
  if (op.kind === undefined || typeof op.kind !== "string") return false;
  switch (op.kind) {
    case "unitary":
      return (
        Array.isArray(op.targets) &&
        op.targets.every(isRegister) &&
        // controls is optional
        (op.controls === undefined ||
          (Array.isArray(op.controls) && op.controls.every(isRegister))) &&
        // isAdjoint is optional
        (op.isAdjoint === undefined || typeof op.isAdjoint === "boolean")
      );
    case "measurement":
      return (
        Array.isArray(op.qubits) &&
        op.qubits.every(isRegister) &&
        Array.isArray(op.results) &&
        op.results.every(isRegister)
      );
    case "ket":
      return Array.isArray(op.targets) && op.targets.every(isRegister);
    default:
      return false;
  }
}

/**
 * A parameter for an operation.
 */
export interface Parameter {
  /** Parameter name. */
  name: string;
  /** Parameter's Q# type. */
  type: string;
}

/**
 * Runtime check: is this a valid Parameter?
 */
export function isParameter(obj: any): obj is Parameter {
  return (
    obj &&
    typeof obj === "object" &&
    typeof obj.name === "string" &&
    typeof obj.type === "string"
  );
}

/**
 * Conditions on when to render the given operation.
 */
export enum ConditionalRender {
  /** Always rendered. */
  Always,
  /** Render classically-controlled operation when measurement is a zero. */
  OnZero,
  /** Render classically-controlled operation when measurement is a one. */
  OnOne,
  /** Render operation as a group of its nested operations. */
  AsGroup,
}

/**
 * Custom data attributes (e.g. data-{attr}="{val}")
 */
export interface DataAttributes {
  [attr: string]: string;
}

export interface SourceLocation {
  file: string;
  line: number;
  column: number;
}

export interface Metadata {
  source?: SourceLocation;
  scopeLocation?: SourceLocation;
}
