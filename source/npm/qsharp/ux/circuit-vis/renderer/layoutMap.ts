// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

/**
 * LayoutMap — exported geometry from the circuit-rendering pass.
 *
 * [`processOperations`](process.ts) computes every coordinate while laying out the circuit. Rather
 * than discard those numbers into SVG attributes and reverse-engineer them later, the renderer
 * captures them in a `LayoutMap` and passes it to the editor directly — one source of truth,
 * accurate for nested scopes.
 *
 * The map is owned by the View layer (regenerated on every render) and consumed by the editor
 * controllers. Editor mutations go through the Action layer, which has no knowledge of `LayoutMap`.
 */

/**
 * Geometry for one *scope* — either the top-level component grid or the children grid of one
 * expanded group. Each `processOperations` call (top-level or recursive via `_processChildren`)
 * corresponds to exactly one scope.
 */
export type LayoutScope = {
  /**
   * Absolute x of the left edge of each column in this scope, indexed by column index within the
   * scope. "Left edge" is where a gate centered in the column has its bounding box's left edge —
   * i.e. `colStartX[i]` from `_fillRenderDataX`, which already accounts for `gatePadding` between
   * columns.
   */
  columnXOffsets: number[];

  /**
   * Width of each column. `columnXOffsets[i] + columnWidths[i]` gives the right edge of column
   * `i`'s gates (before the inter-column `gatePadding * 2`).
   */
  columnWidths: number[];

  /**
   * Absolute x of the right edge of the group box enclosing this scope, if any. Set only for a
   * nested group's children scope; `undefined` for the top-level scope (which has no enclosing
   * box). A long group label can stretch the box wider than its child columns, so this can sit
   * right of `columnXOffsets[last] + columnWidths[last]`. The editor uses it to widen the scope's
   * interior trailing dropzone to cover that label slack.
   */
  boxRightX?: number;
};

/**
 * Complete geometry for a rendered circuit. Built by [`processOperations`](process.ts) and threaded
 * through [`Sqore.compose`](sqore.ts) to the editor.
 */
export type LayoutMap = {
  /**
   * Per-scope geometry, keyed by the *parent operation's* location string. The top-level scope (the
   * circuit's root component grid) is keyed by `""`; the children of an expanded group at location
   * `"0,0"` are keyed by `"0,0"`; grandchildren by `"0,0-1,2"`; etc. This is the addressing
   * convention used by [`findParentArray`](utils.ts), so a `data-dropzone-location` of `"0,0-1,2"`
   * points at scope `"0,0"`, column 1, opIndex 2.
   */
  scopes: Map<string, LayoutScope>;

  /**
   * Y coord of each *real* qubit wire, indexed by qubit id. Mirrors the values
   * [`getWireData`](../editor/domUtils.ts) recovers from the DOM, but captured at compose time
   * before any editor chrome (e.g. the ghost qubit wire) is added.
   */
  wireYs: number[];
};

/** Construct an empty `LayoutMap`. */
export const emptyLayoutMap = (): LayoutMap => ({
  scopes: new Map(),
  wireYs: [],
});
