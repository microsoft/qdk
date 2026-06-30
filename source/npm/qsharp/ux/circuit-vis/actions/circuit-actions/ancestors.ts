// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { ComponentGrid, Operation } from "../../data/circuit.js";
import { CircuitModel } from "../../data/circuitModel.js";
import { Location } from "../../data/location.js";
import { findOperation, findParentArray } from "../../utils.js";

/*
 * `ancestors.ts` — ancestor-chain capture for the Action layer.
 *
 * A move/add/remove mutates a deeply nested op, then walks back up
 * the tree to refresh (or prune) every enclosing group. Because
 * mid-mutation column splices invalidate location strings, these
 * helpers capture the chain as `(op, containingArray)` object
 * references BEFORE any mutation — those survive structural edits a
 * location string can't. Depends on the Data layer and `utils.ts`.
 */

/**
 * One rung of an ancestor chain: an ancestor op paired with the
 * array reference that contains it, captured BEFORE any mutation so
 * both stay valid even as column splices invalidate location
 * strings.
 */
type AncestorRung = { op: Operation; containingArray: ComponentGrid };

/**
 * Collect the ancestor chain of the op (or slot) at `location`,
 * innermost-first, up to (but not including) the root grid. The
 * chain is the ancestors of `location`'s parent; the op/slot at
 * `location` itself is never included.
 *
 * Both `moveOperation` call sites use this: `sourceLocation` feeds
 * the empty-group cleanup (the source op is already detached by
 * `removeOp`), and `targetLocation` feeds the dest-side refresh
 * (its parent is the dropzone's scope op). Captured rungs are object
 * references, so they survive column splices that invalidate
 * location strings.
 */
const collectAncestorChain = (
  model: CircuitModel,
  location: string,
): AncestorRung[] => {
  const chain: AncestorRung[] = [];
  let loc = Location.parse(location).parent();
  while (!loc.isRoot) {
    const locStr = loc.toString();
    const op = findOperation(model.componentGrid, locStr);
    const containingArray = findParentArray(model.componentGrid, locStr);
    if (op == null || containingArray == null) break;
    chain.push({ op, containingArray });
    loc = loc.parent();
  }
  return chain;
};

/**
 * Walk the tree and collect the ancestor chain (innermost-first, up
 * to but not including the root grid) leading to `target`, comparing
 * by object identity. Used by mutators like
 * [`addControl`](circuitActions.ts)/[`removeControl`](circuitActions.ts)
 * that hold an `Operation` reference but no location. Returns `[]`
 * if `target` is top-level or not found (callers treat both the
 * same: no ancestors to refresh).
 */
const findAncestorChainForOp = (
  model: CircuitModel,
  target: Operation,
): AncestorRung[] => {
  const walk = (
    grid: ComponentGrid,
    chain: AncestorRung[],
  ): AncestorRung[] | null => {
    for (const col of grid) {
      for (const op of col.components) {
        if (op === target) return chain;
        if (op.children != null) {
          const found = walk(op.children, [
            { op, containingArray: grid },
            ...chain,
          ]);
          if (found != null) return found;
        }
      }
    }
    return null;
  };
  return walk(model.componentGrid, []) ?? [];
};

/**
 * Like [`findAncestorChainForOp`](#) but also captures the op's own
 * rung. `opRung.containingArray` is the grid one level above the op
 * (the model's top-level grid for a top-level op, else the parent
 * group's `children`). Used by widening mutators (`addControl`) that
 * feed [`resolveSpanChange`](derivedTargets.ts). Returns `null` if
 * the op isn't in the model.
 */
const findOpRungAndAncestors = (
  model: CircuitModel,
  target: Operation,
): { opRung: AncestorRung; ancestorChain: AncestorRung[] } | null => {
  const walk = (
    grid: ComponentGrid,
    chain: AncestorRung[],
  ): { opRung: AncestorRung; ancestorChain: AncestorRung[] } | null => {
    for (const col of grid) {
      for (const op of col.components) {
        if (op === target) {
          return {
            opRung: { op, containingArray: grid },
            ancestorChain: chain,
          };
        }
        if (op.children != null) {
          const found = walk(op.children, [
            { op, containingArray: grid },
            ...chain,
          ]);
          if (found != null) return found;
        }
      }
    }
    return null;
  };
  return walk(model.componentGrid, []);
};

export type { AncestorRung };
export { collectAncestorChain, findAncestorChainForOp, findOpRungAndAncestors };
