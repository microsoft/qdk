// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { ComponentGrid, Operation } from "../../data/circuit.js";
import { CircuitModel } from "../../data/circuitModel.js";
import { Location } from "../../data/location.js";
import { findOperation, findParentArray } from "../../utils.js";

/*
 * `ancestors.ts` — ancestor-chain capture for the Action layer.
 *
 * A move/add/remove mutates a deeply nested op, then needs to walk
 * back up the tree to refresh (or prune) every enclosing group. The
 * tricky part is that mid-mutation column splices invalidate
 * hierarchical location strings, so these helpers capture the chain
 * as `(op, containingArray)` **object references** BEFORE any
 * mutation — those survive the structural edits a location string
 * can't. Depends only on the Data layer and `utils.ts`.
 */

/**
 * Type alias for one rung of the source op's ancestor chain
 * captured for the empty-group cleanup pass. Each entry pairs an
 * ancestor operation with the array reference that contains it,
 * captured BEFORE the move mutates anything. Both references stay
 * valid through `moveX` / `removeOp` because we hold the array
 * itself, not a location string into it.
 */
type AncestorRung = { op: Operation; containingArray: ComponentGrid };

/**
 * Collect the source op's ancestor chain, innermost-first, up to
 * (but not including) the root grid. Used by the empty-group
 * cleanup pass in `moveOperation`.
 *
 * Walks location strings during capture (before any mutation), but
 * the captured references are object references — they remain
 * valid even if `moveX` later invalidates location strings by
 * splicing columns elsewhere in the tree.
 */
const collectAncestorChain = (
  model: CircuitModel,
  sourceLocation: string,
): AncestorRung[] => {
  const chain: AncestorRung[] = [];
  // Source's PARENT is the innermost ancestor. We never include the
  // source op itself; that's already removed by `removeOp`.
  let loc = Location.parse(sourceLocation).parent();
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
 * Collect the destination's ancestor chain, innermost-first, up to
 * (but not including) the root grid. Used by the dest-side
 * ancestor refresh cascade at the tail of `moveOperation`.
 *
 * `targetLocation` here addresses the *slot* the source is about
 * to be inserted into (e.g. `"0,0-1,0"` = "put me in scope 0,0 at
 * column 1, opIndex 0"). The innermost destination ancestor is the
 * op the dropzone's scope belongs to — i.e. the op at the prefix
 * before the last `-` (`"0,0"` in the example). We walk up from
 * there.
 *
 * Same object-reference contract as `collectAncestorChain`: post-
 * move, `targetLocation` may name a different op (column splices,
 * empty-prune cascades), so we lock in references now.
 */
const collectDestAncestorChain = (
  model: CircuitModel,
  targetLocation: string,
): AncestorRung[] => {
  const chain: AncestorRung[] = [];
  // Innermost destination ancestor = the scope op of the dropzone.
  // `targetLocation`'s parent IS that scope op's location.
  let loc = Location.parse(targetLocation).parent();
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
 * Walk the circuit tree and collect the ancestor chain (innermost-
 * first, up to but not including the root grid) that leads to the
 * specified `target` op — comparing by object identity, not by
 * location string. Used by mutators like
 * [`addControl`](circuitActions.ts)/[`removeControl`](circuitActions.ts)
 * that receive an `Operation` reference but no location.
 *
 * Returns `[]` if `target` is a top-level op (or not found at all
 * — callers should treat "not found" identically to "top-level":
 * either way, there are no ancestors to refresh).
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
 * Like [`findAncestorChainForOp`](#) but ALSO captures the op's
 * own rung. Returned `opRung.containingArray` is the grid one
 * level above the op — i.e. the model's top-level grid for a
 * top-level op, or the parent group's `children` grid otherwise.
 *
 * Used by widening mutators (`addControl` and similar) that
 * receive an `Operation` reference and need to feed
 * [`resolveSpanChange`](derivedTargets.ts).
 *
 * Returns `null` if the op isn't in the model. Callers should
 * treat that as a no-op (the mutation predicate can't fire on a
 * detached op).
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
export {
  collectAncestorChain,
  collectDestAncestorChain,
  findAncestorChainForOp,
  findOpRungAndAncestors,
};
