// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

/**
 * LayoutMap — exported geometry from the circuit-rendering pass.
 *
 * Background: historically the circuit editor recovered geometry by
 * reading `data-width` and `x` attributes off already-rendered SVG
 * host elements. That recovery was approximate and broke for nested
 * scopes — see the "Architecture refactor" section in
 * [CIRCUIT_EDITOR_TODO.md](CIRCUIT_EDITOR_TODO.md), root cause #1.
 *
 * `LayoutMap` is the fix: [`processOperations`](process.ts) already
 * computes every coordinate exactly. Rather than discard those numbers
 * into SVG attributes and reverse-engineer them later, we capture them
 * in a `LayoutMap` and pass it to the editor directly. Same numbers,
 * one source of truth.
 *
 * The map is owned by the View layer (it's regenerated on every render)
 * and consumed by editor controllers (R5). Editor mutations go through
 * the Action layer (R3), which has no knowledge of `LayoutMap`.
 */

/**
 * Geometry for one *scope* — either the top-level component grid or
 * the children grid of one expanded group.
 *
 * "Scope" here matches the recursion structure of
 * [`processOperations`](process.ts): each call to `processOperations`
 * (top-level or recursive via `_processChildren`) corresponds to
 * exactly one scope.
 */
export type LayoutScope = {
  /**
   * Absolute x of the left edge of each column in this scope. Indexed
   * by column index within the scope (i.e. `columnXOffsets[1]` is the
   * left edge of column 1 of the scope, regardless of whether the scope
   * is the top-level grid or a nested group's children).
   *
   * "Left edge" here means the x where a gate centered in the column
   * would have its bounding box's left edge — i.e. `colStartX[i]` from
   * `_fillRenderDataX`, which already accounts for `gatePadding`
   * between columns.
   */
  columnXOffsets: number[];

  /**
   * Width of each column. `columnXOffsets[i] + columnWidths[i]` gives
   * the right edge of column `i`'s gates (before the inter-column
   * `gatePadding * 2`).
   */
  columnWidths: number[];
};

/**
 * Complete geometry for a rendered circuit. Built by
 * [`processOperations`](process.ts) and threaded through
 * [`Sqore.compose`](sqore.ts) to the editor.
 */
export type LayoutMap = {
  /**
   * Per-scope geometry, keyed by the *parent operation's* location
   * string. The top-level scope (the circuit's root component grid)
   * is keyed by `""`; the children of an expanded group at location
   * `"0,0"` are keyed by `"0,0"`; grandchildren by `"0,0-1,2"`; etc.
   *
   * This is exactly the addressing convention used by
   * [`findParentArray`](utils.ts) to navigate the data, so a
   * `data-dropzone-location` of `"0,0-1,2"` points at scope `"0,0"`,
   * column 1, opIndex 2.
   */
  scopes: Map<string, LayoutScope>;

  /**
   * Y coord of each *real* qubit wire, indexed by qubit id. Mirrors
   * the values that [`getWireData`](utils.ts) recovers from the DOM,
   * but captured at compose time before any editor chrome (e.g. the
   * ghost qubit wire) is added.
   */
  wireYs: number[];
};

/** Construct an empty `LayoutMap`. */
export const emptyLayoutMap = (): LayoutMap => ({
  scopes: new Map(),
  wireYs: [],
});
