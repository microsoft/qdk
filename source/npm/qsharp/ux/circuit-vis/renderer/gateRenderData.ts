// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { DataAttributes } from "../data/circuit.js";
import { LayoutScope } from "./layoutMap.js";
import { Register } from "../data/register.js";

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
  /** Whether this group gate is currently expanded. Always false for non-group gates. */
  isExpanded: boolean;
  /** Centre x coord for gate position. */
  x: number;
  /** Array of y coords of control registers. */
  controlsY: number[];
  /** Array of y coords of target registers.
   *  For `GateType.Unitary` or `GateType.ControlledUnitary`, this is an array of groups of
   *  y coords, where each group represents a unitary box to be rendered separately.
   */
  targetsY: (number | number[])[];
  /** Gate label. */
  label: string;
  /** Gate arguments as string. */
  displayArgs?: string;
  /** Gate width. */
  width: number;
  /** Children operations as part of group. */
  children?: GateRenderData[][];
  /** Vertical space from the top of this gate to the top of the topmost contained gate. 0 for non-groups. */
  topPadding: number;
  /** Vertical space from the bottom of this gate to the bottom of the bottommost contained gate. 0 for non-groups. */
  bottomPadding: number;
  /** Custom data attributes to attach to gate element. */
  dataAttributes?: DataAttributes;
  /** Link href and title for clickable gate. */
  link?: { href: string; title: string };
  /**
   * Labels for the classical control registers (when present, this op
   * has at least one classical control). Aligned with `controlsY` by
   * index: a numeric entry is a classical control with a known id, a
   * `null` entry is a classical control whose id couldn't be resolved
   * (B1), and an `undefined` entry marks a QUANTUM control that
   * happens to share the op's `controls` array with classical refs
   * (possible after B5's add-control-on-classical-op fix). The
   * formatter uses the `undefined` entries to route those controls
   * through the standard control-dot render path instead of the
   * classical-circle path.
   */
  classicalControlIds?: (number | null | undefined)[];
  /**
   * Classical control registers used by this operation or any descendant.
   * Used by processOperations to decide which classical wires may pass through
   * this gate body without forcing a split.
   */
  classicalControlRegs?: Register[];
  /**
   * @internal Used during layout to surface child-scope geometry from
   * `_processChildren` up to the parent's `_fillRenderDataX`. Cleared
   * (set to `undefined`) once consumed. Not used outside `process.ts`.
   *
   * Holds the recursive `processOperations` call's `localScope` (in
   * the child's local startX-anchored coords) and any deeper scopes
   * already absolute. The parent's `_fillRenderDataX` shifts the local
   * scope by the group's `offset` and merges everything into its own
   * absolute scope accumulator.
   */
  _childLayout?: {
    localScope: LayoutScope;
    childScopes: Map<string, LayoutScope>;
  };
}
