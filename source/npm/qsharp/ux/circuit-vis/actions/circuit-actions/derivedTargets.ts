// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { ComponentGrid, Operation } from "../../data/circuit.js";
import { Register } from "../../data/register.js";
import { getOperationRegisters } from "../../utils.js";
import { AncestorRung } from "./ancestors.js";
import { doesOverlap, getMinMaxRegIdx } from "./gridPrimitives.js";

/*
 * `derivedTargets.ts` — the eager `.targets` cache and the
 * ancestor-refresh cascade that keeps it in sync.
 *
 * Every group op carries an authoritative `.targets` (`.results` for
 * measurements) that is the deduped union of its descendants' wires.
 * This module owns the machinery that recomputes that cache, walks
 * it up the ancestor chain, prunes emptied groups, and resolves the
 * sibling-column collisions a widened span can introduce. The
 * decision to keep this cache eager (rather than deriving on read)
 * is documented in
 * [`circuitTargets.bench.md`](../../../test/circuit-editor/circuitTargets.bench.md).
 *
 * Depends on `gridPrimitives` (span/overlap checks) and the
 * `AncestorRung` type from `ancestors`; no DOM.
 */

/**
 * `true` if `op` has no rendered content underneath it — either no
 * `children` at all, or `children` that are all empty columns. An
 * empty group has no semantic meaning (nothing to execute) and no
 * sensible render (zero width, zero height); the cleanup pass in
 * `moveOperation` deletes such groups rather than leaving them as
 * landmines for the renderer.
 */
const _isOperationEmpty = (op: Operation): boolean => {
  if (op.children == null || op.children.length === 0) return true;
  return op.children.every((col) => col.components.length === 0);
};

/**
 * Dedup a `Register[]` by full `(qubit, result)` identity, returning
 * fresh `Register` objects in canonical `(qubit, result)` order:
 * qubit-only refs (`result === undefined`) sort before that qubit's
 * classical-result refs, then classical refs by ascending `result`.
 *
 * Bare-qubit `{qubit:N}` and classical-ref `{qubit:N, result:M}` are
 * distinct identities; both survive. Outputs are fresh objects so
 * callers can assign directly into `op.targets` / `op.results`.
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
 * Compute `op`'s derived `.targets` (or `.results`) from the
 * union of its **immediate children's** contributions.
 *
 * Each child contributes the union of its own register-bearing
 * fields (`getOperationRegisters`: targets + controls for
 * unitaries, qubits + results for measurements, targets for
 * kets). Because every group's `.targets` is itself the eager
 * cache of its own subtree, taking only the immediate children
 * here gives the same answer as recursively walking the
 * subtree — but without paying the recursion cost at every
 * ancestor level.
 *
 * Returns `[]` when `op` has no children grid; callers should
 * not invoke this on leaf ops.
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
 * Order-sensitive equality on `Register[]`. Used by the
 * ancestor cascade to decide whether a refresh actually changed
 * the cached value — positional compare is safe because
 * `_computeDerivedTargets` produces stable child-iteration-order
 * output.
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
 * Refresh the derived `.targets` / `.results` of `op` from its
 * immediate children. Returns `true` iff the cached value
 * actually changed — the ancestor cascade in
 * `refreshAncestorTargets` uses this signal to terminate.
 *
 * No-op (returns `false`) for ops with no `children` grid;
 * those are leaf ops whose `.targets` is explicit, not derived.
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
 * [`findAndRemoveOperations`](circuitActions.ts) that may have
 * stripped ops from many different ancestor chains in one pass —
 * too many separate chains to track individually, so we just
 * re-derive every group's cache in a single bottom-up sweep.
 *
 * Post-order is essential: a parent's cache is recomputed from
 * its immediate children's `.targets`, which must already reflect
 * the post-mutation state. Walking bottom-up ensures children are
 * refreshed before any parent reads them.
 *
 * Narrowing-only — no overlap-resolver pass. Batch removal can
 * only shrink ancestor spans, never widen them, so no new
 * sibling-column collisions can appear.
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
 * After `refreshDerivedTargets` widens `op`'s `.targets`, its
 * register span may now overlap one or more siblings in the same
 * column. The renderer can't draw two ops on the same column whose
 * spans intersect, so we follow the existing
 * [`commitAddControl`](../../editor/controllers/dragController.ts)
 * convention: splice `op` out of its current column and insert a
 * brand-new column containing only `op` at the SAME column index.
 * That pushes the old column (with the surviving siblings) one
 * slot to the right of `op`, restoring a non-overlapping layout
 * without disturbing any siblings' relative order.
 *
 * No-op when:
 *   - the column has no siblings (nothing to overlap),
 *   - no sibling actually overlaps `op`'s (possibly widened) span,
 *   - `op` can't be located in `containingArray` (e.g. it was
 *     already pruned upstream — defensive guard, shouldn't fire).
 *
 * Symmetric to `resolveOverlappingOperations` but operates on a
 * single known op rather than scanning the whole grid: the cascade
 * already knows which op was just widened and which array contains
 * it, so a targeted resolve is cheaper than a full grid sweep.
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
 * still-attached ancestor's derived `.targets` / `.results`
 * from its immediate children. Stops at the first ancestor
 * whose cached value didn't actually change — at that point
 * nothing higher up can change either, because every higher
 * ancestor's cache depends only on its own immediate
 * children's caches.
 *
 * This is the single centralized mechanism for keeping the eager
 * `.targets` cache in sync with the children, used by both the
 * source-side post-prune refresh and the dest-side post-insert
 * refresh in `moveOperation`.
 *
 * # Why immediate children only
 *
 * Each child's `.targets` is **already** the correct eager
 * cache of its own subtree — every prior refresh maintained
 * that invariant. So a parent's union is just the union of its
 * immediate children's contributions, no recursion needed. The
 * naive alternative (`getChildTargets` walks the whole subtree
 * top-down on every ancestor) costs O(subtree-size) per
 * ancestor, which compounds to O(depth × subtree) across the
 * chain — wasted work, because each level repeats what the
 * level below it already finished.
 *
 * # Contract
 *
 *   - `chain` is innermost-first: index 0 is the deepest
 *     ancestor, last index is closest to the root grid.
 *   - Rungs whose `op` is no longer attached to its captured
 *     `containingArray` are silently skipped — the empty-prune
 *     pass may have removed them between capture and refresh.
 *     The walk continues to the next-attached rung (whose
 *     children list changed because the detached rung was
 *     removed).
 *   - Refresh is **deterministic and idempotent**: calling this
 *     twice in a row on the same chain has no observable effect
 *     on the second call (every refresh on the second pass
 *     returns "unchanged" immediately).
 *   - The walk does **no DOM work, no column reshape, no
 *     model-level structural edits**. Callers that need to
 *     react to a widened span — e.g. split a column when an
 *     ancestor's new span now overlaps a sibling — pass
 *     `onAfterRefresh` to compose that on top. The hook fires
 *     on **every** visited still-attached rung, regardless of
 *     whether the refresh changed `.targets`: a shared
 *     ancestor between source and dest chains can have its
 *     value written by the first cascade to reach it, leaving
 *     the second to see "no change" — but the span still
 *     widened relative to pre-mutation state, so the hook
 *     (e.g. overlap resolver) must still run. The hook is
 *     expected to be idempotent under that no-op case.
 *
 * # Why the chain is captured before mutation
 *
 * Callers (`collectAncestorChain` / `collectDestAncestorChain`)
 * walk `Location` strings during capture, then hold the resulting
 * `(op, containingArray)` object references for the duration of
 * the mutation. Hierarchical location strings can be invalidated
 * by mid-move column splices or empty-prune cascades; object
 * references survive those shifts because they identify the
 * actual array and op without traversing the tree.
 *
 * # Termination is symmetric across widening and narrowing
 *
 * Works for both narrowing (source-side: a child was removed,
 * so spans may shrink) and widening (dest-side: a child was
 * inserted, so spans may grow) refreshes. The
 * cache-value-unchanged check fires whenever this ancestor's
 * recomputed value matches its prior cache — that's the
 * "nothing higher up can change" stop condition for both
 * directions, since every higher ancestor reads only its own
 * immediate children's `.targets`.
 */
const refreshAncestorTargets = (
  chain: AncestorRung[],
  options: { onAfterRefresh?: (rung: AncestorRung) => void } = {},
): void => {
  for (const rung of chain) {
    const { op, containingArray } = rung;

    // Skip rungs that were detached between capture and refresh
    // (e.g. the empty-prune pass spliced them out). Keep walking —
    // the next-attached ancestor's children list changed because
    // the detached rung was removed, so it still needs refreshing.
    let stillAttached = false;
    for (const col of containingArray) {
      if (col.components.indexOf(op) >= 0) {
        stillAttached = true;
        break;
      }
    }
    if (!stillAttached) continue;

    const changed = refreshDerivedTargets(op);
    // Hook fires on every visited still-attached rung — see
    // the "shared ancestor" note in the doc comment for why
    // this can't be gated on `changed`.
    options.onAfterRefresh?.(rung);
    // Early-exit the refresh walk once nothing higher up can
    // change. EXCEPTION: when `onAfterRefresh` is provided, walk
    // the FULL chain so the hook fires on every ancestor. The
    // canonical case is `moveOperation`: its source-side cascade
    // (no hook) propagates the new spans up through every shared
    // ancestor, so the subsequent dest-side cascade (with hook)
    // sees `!changed` at the very first rung and would otherwise
    // skip the overlap-resolver on the higher ancestors where
    // the actual collision lives. The doc-comment contract
    // ("hook fires on every visited still-attached rung") is
    // only meaningful if we keep walking past the no-change rung.
    if (!changed && options.onAfterRefresh == null) return;
  }
};

/**
 * Centralized post-widening cleanup for any single op whose
 * register span just changed (added control, added target, body
 * widened by a wire remap, etc.). Use this whenever a mutation
 * widens a specific op's `.targets` / `.controls` — it's the
 * single chokepoint so the "after I changed this op, make sure
 * everyone moves out of the way" invariant can't be missed at a
 * call site.
 *
 * Two-stage cleanup:
 *
 *   1. **Op itself.** Check the widened op against its own
 *      column siblings; if it now collides, splice it into a
 *      fresh column at the same index (same convention as the
 *      ancestor cascade below). This is the case the ancestor-
 *      only cascade misses: a top-level `addControl` that widens
 *      the op into a same-column sibling has NO ancestors, so
 *      the existing `onAfterRefresh` hook never fires. Same
 *      symptom at any nesting level — the immediate-children
 *      array's collisions are nobody's problem under the
 *      ancestor-only model.
 *   2. **Ancestor chain.** Re-derive each ancestor's `.targets`
 *      cache; if that widened the ancestor's span, resolve its
 *      own column collision via the same hook. Identical to
 *      what the existing `refreshAncestorTargets` cascade does;
 *      this routine just wraps the standard hook so every
 *      widening path uses it uniformly.
 *
 * Idempotent on the no-collision path: stage 1 is a one-pass
 * scan + no-op return; stage 2 early-exits on the first ancestor
 * whose cache didn't change. Safe to call from paths whose
 * earlier work (e.g. `addOp`'s pre-insert overlap check) may
 * have already resolved the collision — the redundant call is
 * cheap and the architectural guarantee is worth more than the
 * micro-cost.
 *
 * @param opRung  The op being widened paired with its containing
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
 * whose children just collapsed to empty. Deletion is cascading:
 * if removing an ancestor empties its grandparent (because that
 * grandparent only contained this one ancestor), the grandparent
 * gets deleted next.
 *
 * Why this is necessary. `moveOperation` removes the source op
 * from its parent's grid, but the parent group itself is left in
 * place even if it now has no children. The renderer trips on
 * empty groups (zero-wire layout, undefined targets), and even if
 * it didn't, an empty group has no semantic meaning — there's
 * nothing to execute, no Q# to emit. Quietly deleting it matches
 * what users intuitively expect when they "move the last thing
 * out of" a group.
 *
 * Returns the surviving (still-attached) rungs of the chain in
 * innermost-first order. The caller hands this back to
 * `refreshAncestorTargets` so the first non-empty ancestor — and
 * any higher ancestors whose span still doesn't enclose what's
 * below it — get their derived `.targets` updated to reflect the
 * post-prune child set.
 *
 * Note on `qubitUseCounts` drift. We don't adjust use-counts when
 * deleting an empty group because the group itself contributed no
 * use-count entries (the count is per leaf-op, and the empty group
 * has no leaves). `removeTrailingUnusedQubits` is the safety net.
 */
const pruneEmptyAncestors = (chain: AncestorRung[]): AncestorRung[] => {
  const survived: AncestorRung[] = [];
  let stillPruning = true;
  for (const rung of chain) {
    if (!stillPruning) {
      // Above the first non-empty ancestor — nothing here can
      // have been emptied by our move, but keep these rungs
      // around so the post-prune refresh walk can early-exit
      // through them cleanly.
      survived.push(rung);
      continue;
    }
    if (!_isOperationEmpty(rung.op)) {
      // First non-empty rung terminates the prune phase. It
      // survives (its children changed; refresh handles its
      // .targets) and so does everything above it.
      stillPruning = false;
      survived.push(rung);
      continue;
    }
    // Splice rung.op out of its containing array. Mirror the
    // column-cleanup convention from `removeOp`: if the column
    // that held `op` is now empty, drop the column too.
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
