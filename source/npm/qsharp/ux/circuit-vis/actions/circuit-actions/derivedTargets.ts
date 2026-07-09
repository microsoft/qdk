// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { ComponentGrid, Operation } from "../../data/circuit.js";
import { Register } from "../../data/register.js";
import { getMinMaxRegIdx, getOperationRegisters } from "../../utils.js";
import { AncestorRung } from "./ancestors.js";
import { doesOverlap } from "./gridPrimitives.js";

/*
 * `derivedTargets.ts` — the eager `.targets` cache and the
 * ancestor-refresh cascade that keeps it in sync.
 *
 * Every group op carries an authoritative `.targets` (`.results` for
 * measurements) that is the deduped union of its descendants' wires.
 * This module recomputes that cache, walks it up the ancestor chain,
 * prunes emptied groups, and resolves the sibling-column collisions a
 * widened span can introduce. The eager cache keeps `.targets`
 * authoritative so the renderer and resolver read wire spans in O(1)
 * instead of walking descendants on every query.
 *
 * Depends on `gridPrimitives` and the `AncestorRung` type; no DOM.
 */

/**
 * `true` if `op` has no rendered content — no `children`, or only
 * empty columns. The cleanup pass in `moveOperation` deletes such
 * groups (no semantics, no sensible render).
 */
const _isOperationEmpty = (op: Operation): boolean => {
  if (op.children == null || op.children.length === 0) return true;
  return op.children.every((col) => col.components.length === 0);
};

/**
 * Dedup a `Register[]` by full `(qubit, result)` identity, returning
 * fresh `Register` objects in canonical order: qubit-only refs sort
 * before that qubit's classical-result refs, then classical refs by
 * ascending `result`. Bare-qubit and classical-ref entries are
 * distinct identities; both survive.
 */
const _dedupRegistersByIdentity = (registers: Register[]): Register[] => {
  const seen = new Set<string>();
  const out: Register[] = [];
  for (const reg of registers) {
    const key =
      reg.result === undefined
        ? `${reg.qubit}:q`
        : `${reg.qubit}:c${reg.result}`;
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(
      reg.result === undefined
        ? { qubit: reg.qubit }
        : { qubit: reg.qubit, result: reg.result },
    );
  }
  out.sort((a, b) => {
    if (a.qubit !== b.qubit) return a.qubit - b.qubit;
    if (a.result === undefined && b.result === undefined) return 0;
    if (a.result === undefined) return -1;
    if (b.result === undefined) return 1;
    return a.result - b.result;
  });
  return out;
};

/**
 * Compute `op`'s derived `.targets` (or `.results`) from the union
 * of its immediate children's register-bearing fields. Each group's
 * `.targets` is already the eager cache of its own subtree, so the
 * immediate children suffice — no recursion. Returns `[]` when `op`
 * has no children grid.
 */
const _computeDerivedTargets = (op: Operation): Register[] => {
  if (op.children == null) return [];
  const registers: Register[] = [];
  for (const col of op.children) {
    for (const child of col.components) {
      registers.push(...getOperationRegisters(child));
    }
  }
  return _dedupRegistersByIdentity(registers);
};

/**
 * Order-sensitive equality on `Register[]`. The ancestor cascade
 * uses it to decide whether a refresh changed the cached value;
 * positional compare is safe because `_computeDerivedTargets`
 * produces stable-order output.
 */
const _registerListsEqual = (a: Register[], b: Register[]): boolean => {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) {
    if (a[i].qubit !== b[i].qubit) return false;
    if (a[i].result !== b[i].result) return false;
  }
  return true;
};

/**
 * Refresh `op`'s derived `.targets` / `.results` from its immediate
 * children. Returns `true` iff the cached value changed — the
 * ancestor cascade uses this signal to terminate. No-op (`false`)
 * for leaf ops with no `children` grid.
 */
const refreshDerivedTargets = (op: Operation): boolean => {
  if (op.children == null) return false;
  const newValue = _computeDerivedTargets(op);
  if (op.kind === "measurement") {
    if (_registerListsEqual(op.results, newValue)) return false;
    op.results = newValue;
    return true;
  }
  if (op.kind === "unitary" || op.kind === "ket") {
    if (_registerListsEqual(op.targets, newValue)) return false;
    op.targets = newValue;
    return true;
  }
  return false;
};

/**
 * Post-order deep refresh of every group's derived `.targets` /
 * `.results` in `grid`. Used by batch mutators like
 * [`removeQubitWithDependents`](circuitActions.ts) that strip ops
 * from many ancestor chains at once. Post-order is essential: a
 * parent is recomputed from its children's caches, which must already
 * reflect the post-mutation state. Narrowing-only — batch removal
 * can't widen spans, so no new collisions appear.
 */
const deepRefreshDerivedTargets = (grid: ComponentGrid): void => {
  for (const col of grid) {
    for (const op of col.components) {
      if (op.children != null) {
        deepRefreshDerivedTargets(op.children);
        refreshDerivedTargets(op);
      }
    }
  }
};

/**
 * Sibling-collision resolver for the extend cascade.
 *
 * After a refresh widens `op`'s `.targets`, its span may overlap a
 * sibling in the same column. Following the
 * [`commitAddControl`](../../editor/controllers/dragController.ts)
 * convention, splice `op` out and insert a fresh column containing
 * only `op` at the same index — this pushes the surviving siblings
 * one slot right, restoring a non-overlapping layout.
 *
 * No-op when the column has no siblings, no sibling overlaps `op`,
 * or `op` isn't found in `containingArray`. Symmetric to
 * `resolveOverlappingOperations` but targets a single known op
 * rather than scanning the whole grid.
 */
const _resolveOverlapAfterExtend = (
  op: Operation,
  containingArray: ComponentGrid,
): void => {
  // Locate `op` in `containingArray`.
  let columnIndex = -1;
  let position = -1;
  for (let c = 0; c < containingArray.length; c++) {
    const idx = containingArray[c].components.indexOf(op);
    if (idx >= 0) {
      columnIndex = c;
      position = idx;
      break;
    }
  }
  if (columnIndex < 0) return;

  const column = containingArray[columnIndex];
  if (column.components.length <= 1) return;

  const [opMin, opMax] = getMinMaxRegIdx(op);
  let collides = false;
  for (let i = 0; i < column.components.length; i++) {
    if (i === position) continue;
    const [sMin, sMax] = getMinMaxRegIdx(column.components[i]);
    if (doesOverlap([opMin, opMax], [sMin, sMax])) {
      collides = true;
      break;
    }
  }
  if (!collides) return;

  // Splice `op` out and insert a fresh column containing only `op`
  // at the same index — this pushes the surviving siblings one
  // column to the right of `op`.
  column.components.splice(position, 1);
  containingArray.splice(columnIndex, 0, { components: [op] });
};

/**
 * Walk an ancestor chain innermost-out and refresh each
 * still-attached ancestor's derived `.targets` / `.results` from
 * its immediate children. Stops at the first ancestor whose cached
 * value didn't change — nothing higher up can change either, since
 * every ancestor's cache depends only on its own immediate
 * children's caches. This is the single mechanism keeping the eager
 * cache in sync, used by both the source-side and dest-side
 * cascades in `moveOperation`.
 *
 * Contract:
 *   - `chain` is innermost-first (index 0 is the deepest ancestor).
 *   - Rungs whose `op` is detached from its captured
 *     `containingArray` are skipped (the empty-prune pass may have
 *     removed them between capture and refresh); the walk continues.
 *   - Refresh is idempotent: a second pass on the same chain is a
 *     no-op.
 *   - Does no DOM work or structural edits. Callers reacting to a
 *     widened span (e.g. column-split on overlap) pass
 *     `onAfterRefresh`. The hook fires on EVERY visited
 *     still-attached rung, even when `.targets` didn't change: a
 *     shared ancestor between source and dest chains may already
 *     have been written by the first cascade, yet its span still
 *     widened relative to pre-mutation state, so the hook must run.
 *     The hook is expected to be idempotent.
 *
 * The chain is captured (as `(op, containingArray)` object
 * references) before mutation by `collectAncestorChain` /
 * `findOpRungAndAncestors`, because mid-move column splices can
 * invalidate location strings while object references survive.
 *
 * Works for both narrowing (source-side child removed) and widening
 * (dest-side child inserted) refreshes; the cache-unchanged check is
 * the stop condition for both directions.
 */
const refreshAncestorTargets = (
  chain: AncestorRung[],
  options: { onAfterRefresh?: (rung: AncestorRung) => void } = {},
): void => {
  for (const rung of chain) {
    const { op, containingArray } = rung;

    // Skip rungs detached between capture and refresh (e.g. spliced
    // out by the empty-prune pass). Keep walking — the next-attached
    // ancestor's children changed and still needs refreshing.
    let stillAttached = false;
    for (const col of containingArray) {
      if (col.components.indexOf(op) >= 0) {
        stillAttached = true;
        break;
      }
    }
    if (!stillAttached) continue;

    const changed = refreshDerivedTargets(op);
    // Fires on every visited rung (see the "shared ancestor" note
    // in the doc comment for why this can't be gated on `changed`).
    options.onAfterRefresh?.(rung);
    // Early-exit once nothing higher can change. EXCEPTION: when a
    // hook is provided, walk the full chain so it fires on every
    // ancestor. In `moveOperation` the source-side cascade (no hook)
    // already propagated spans up through shared ancestors, so the
    // dest-side cascade sees `!changed` at the first rung but the
    // collision lives higher up.
    if (!changed && options.onAfterRefresh == null) return;
  }
};

/**
 * Centralized post-widening cleanup for any single op whose register
 * span just changed (added control/target, body widened by a remap).
 * Call this whenever a mutation widens a specific op's span — the
 * single chokepoint so "make everyone move out of the way" can't be
 * missed at a call site.
 *
 * Two stages:
 *   1. The op itself — check it against its own column siblings and
 *      split a fresh column on collision. This is the case the
 *      ancestor-only cascade misses (a top-level `addControl` that
 *      collides with a same-column sibling has no ancestors).
 *   2. Ancestor chain — re-derive each ancestor's cache and resolve
 *      its column collision via the same hook.
 *
 * Idempotent on the no-collision path; safe to call redundantly.
 *
 * @param opRung The op being widened paired with its containing
 *   array (the grid one level above the op). Capture BEFORE the
 *   mutation if column splices may follow.
 * @param ancestorChain Innermost-first ancestor rungs above
 *   `opRung.op`. Empty when `opRung.op` is top-level.
 */
const resolveSpanChange = (
  opRung: AncestorRung,
  ancestorChain: AncestorRung[],
): void => {
  _resolveOverlapAfterExtend(opRung.op, opRung.containingArray);
  refreshAncestorTargets(ancestorChain, {
    onAfterRefresh: ({ op, containingArray }) =>
      _resolveOverlapAfterExtend(op, containingArray),
  });
};

/**
 * Walk the ancestor chain innermost-out and delete any ancestor
 * whose children just collapsed to empty. Cascading: if removing an
 * ancestor empties its grandparent, the grandparent goes next.
 *
 * `moveOperation` removes the source op but leaves its now-childless
 * parent group in place; an empty group has no semantics and trips
 * the renderer, so deleting it matches what users expect when they
 * move the last thing out of a group.
 *
 * Returns the surviving rungs (innermost-first); the caller hands
 * them to `refreshAncestorTargets` so the first non-empty ancestor
 * (and any above it) get their derived `.targets` updated.
 *
 * `qubitUseCounts` isn't adjusted here — the empty group contributed
 * no per-leaf counts; `removeTrailingUnusedQubits` is the safety net.
 */
const pruneEmptyAncestors = (chain: AncestorRung[]): AncestorRung[] => {
  const survived: AncestorRung[] = [];
  let stillPruning = true;
  for (const rung of chain) {
    if (!stillPruning) {
      // Above the first non-empty ancestor — nothing here was
      // emptied by our move, but keep the rung so the refresh walk
      // can early-exit through it cleanly.
      survived.push(rung);
      continue;
    }
    if (!_isOperationEmpty(rung.op)) {
      // First non-empty rung ends the prune phase; it and everything
      // above it survive.
      stillPruning = false;
      survived.push(rung);
      continue;
    }
    // Splice rung.op out of its containing array. Mirror `removeOp`'s
    // column-cleanup: if the column is now empty, drop it too.
    for (let colIdx = 0; colIdx < rung.containingArray.length; colIdx++) {
      const col = rung.containingArray[colIdx];
      const opIdx = col.components.indexOf(rung.op);
      if (opIdx >= 0) {
        col.components.splice(opIdx, 1);
        if (col.components.length === 0) {
          rung.containingArray.splice(colIdx, 1);
        }
        break;
      }
    }
    // Don't push the deleted rung to `survived`.
  }
  return survived;
};

export {
  deepRefreshDerivedTargets,
  pruneEmptyAncestors,
  refreshDerivedTargets,
  resolveSpanChange,
  refreshAncestorTargets,
};
