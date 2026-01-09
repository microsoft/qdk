// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { DataAttributes } from "./circuit.js";

/**
 * Enum for the various gate operations handled.
 */
export enum GateType {
  /** Measurement gate. */
  Measure,
  /** CNOT gate. */
  Cnot,
  /** SWAP gate. */
  Swap,
  /** X gate. */
  X,
  /** |0⟩or |1⟩ gate. */
  Ket,
  /** Single/multi qubit unitary gate. */
  Unitary,
  /** Single/multi controlled unitary gate. */
  ControlledUnitary,
  /** Nested group of classically-controlled gates. */
  ClassicalControlled,
  /** Group of nested gates */
  Group,
  /** Invalid gate. */
  Invalid,
}

/**
 * Rendering data used to store information pertaining to a given
 * operation for rendering its corresponding SVG.
 */
export interface GateRenderData {
  /** Gate type. */
  type: GateType;
  /** Centre x coord for gate position. */
  x: number;
  /** Array of y coords of control registers. */
  controlsY: number[];
  /** Array of y coords of target registers.
   *  For `GateType.Unitary` or `GateType.ControlledUnitary`, this is an array of groups of
   *  y coords, where each group represents a unitary box to be rendered separately.
   */
  targetsY: (number | number[])[];
  /** y coord of top of gate row */
  topY: number;
  /** Gate label. */
  label: string;
  /** Gate arguments as string. */
  displayArgs?: string;
  /** Gate width. */
  width: number;
  /** Children operations as part of group. */
  children?: GateRenderData[][] | GateRenderData[][][];
  /** Custom data attributes to attach to gate element. */
  dataAttributes?: DataAttributes;
  /** Link href and title for clickable gate. */
  link?: { href: string; title: string };
}
