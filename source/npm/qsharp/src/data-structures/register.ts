// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

/**
 * Type of register.
 */
export enum RegisterType {
  Qubit,
  Classical,
}

/**
 * Represents a register resource.
 */
export interface Register {
  /** Qubit register ID. */
  qubit: number;
  /** Classical register ID. If present, register is classical register. */
  result?: number;
}

/**
 * Runtime check: is this a valid Register?
 */
export function isRegister(obj: any): obj is Register {
  return (
    obj &&
    typeof obj === "object" &&
    typeof obj.qubit === "number" &&
    // result is optional, but if present must be a number
    (obj.result === undefined || typeof obj.result === "number")
  );
}

/**
 * Rendering data for qubit register.
 */
export interface RegisterRenderData {
  /** Type of register. */
  type: RegisterType;
  /** y coord of top of register row */
  topY: number;
  /** y coord of register */
  wireY: number;
  /** Nested classical registers attached to quantum register. */
  children?: RegisterRenderData[];
  /** Link href and title for clickable gate. */
  link?: { href: string; title: string };
}

/**
 * Mapping from qubit IDs to their register render data.
 */
export interface RegisterMap {
  [id: number]: RegisterRenderData;
}
