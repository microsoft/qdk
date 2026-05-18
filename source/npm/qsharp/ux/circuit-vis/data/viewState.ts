// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { ComponentGrid } from "./circuit.js";

/**
 * Per-session view preferences that survive `Sqore.renderCircuit()`
 * but are NOT persisted to the saved circuit (`.qsc`) file.
 *
 * # The third state layer
 *
 * The editor has three distinct kinds of state, each with a different
 * lifetime:
 *
 * | Layer            | Lifetime                  | Persisted? | Owner             |
 * | ---------------- | ------------------------- | ---------- | ----------------- |
 * | `CircuitModel`   | The circuit's lifetime    | Yes (.qsc) | Action layer      |
 * | `ViewState`      | The editor session        | No         | View layer (Sqore)|
 * | `InteractionState` | A single gesture        | No         | Action layer      |
 *
 * `ViewState` is for things the user expects to remain stable as they
 * edit the circuit but does NOT belong in the file. The motivating
 * case is per-group expand/collapse: the user expands a group, makes
 * a few edits, and reasonably expects the group to still be expanded
 * afterward — but two checkouts of the same `.qsc` from different
 * machines should not differ in expansion state.
 *
 * # Override semantics
 *
 * `ViewState` only stores **explicit user choices**. Default
 * expansion (depth-based via `renderDepth`, single-op auto-expand,
 * classically-controlled groups) is computed per-render in
 * [sqore.ts](../sqore.ts). User overrides are applied after the
 * defaults via [`applyTo`](#method-applyTo), so:
 *
 *   - Absent entry → defaults win (today's behavior, unchanged).
 *   - `true` entry → expanded, even if the default would collapse.
 *   - `false` entry → collapsed, even if the default would expand.
 *
 * # Position stability
 *
 * Entries are keyed by the op's hierarchical location string
 * (e.g. `"0,0-1,2"`), but locations are not stable under edits
 * that splice columns or grids. To keep user overrides attached
 * to the right op as the user drags gates around, the View layer
 * (`Sqore`) snapshots an `op → location` map after every render
 * and calls [`rebase`](#method-rebase) at the start of the next
 * render to migrate keys forward via object identity. See
 * [sqore.ts](../sqore.ts).
 *
 * External tree replacement (`Sqore.updateCircuit`) destroys the
 * identity link between old and new ops; the snapshot is dropped
 * there and the next render starts fresh.
 */
export class ViewState {
  /**
   * Map from op location string to the user's explicit expansion
   * choice. Absent = no choice (use defaults).
   */
  readonly expanded = new Map<string, boolean>();

  /**
   * Record the user's choice to expand or collapse the op at
   * `location`. Idempotent.
   *
   * Collapsing also clears any explicit overrides on descendants of
   * `location` so that re-expanding later doesn't auto-spring
   * previously-expanded children back open. (This matches the
   * original `collapseOperation` semantics in [sqore.ts](../sqore.ts).)
   */
  setExpanded(location: string, expanded: boolean): void {
    this.expanded.set(location, expanded);
    if (!expanded) {
      const descendantPrefix = location + "-";
      for (const key of Array.from(this.expanded.keys())) {
        if (key.startsWith(descendantPrefix)) this.expanded.delete(key);
      }
    }
  }

  /**
   * Drop the user's choice for the op at `location`, falling back
   * to the default-expansion logic on the next render. Idempotent;
   * absence of an entry is fine.
   */
  clearExpanded(location: string): void {
    this.expanded.delete(location);
  }

  /**
   * Rewrite expansion keys via an old → new location mapping.
   *
   * For each existing entry at `oldKey`:
   *   - `remap.get(oldKey) === <string>` → rekey to that string
   *     (drop `oldKey`, set the new key to the same value).
   *   - `remap.get(oldKey) === null` → drop the entry (op is no
   *     longer present in the grid).
   *   - `remap.has(oldKey) === false` → leave unchanged (the caller
   *     had no information about this op; safest is to keep the
   *     entry rather than guess).
   *
   * The View layer ([sqore.ts](../sqore.ts)) calls this on every
   * render to track ops whose locations shifted due to upstream
   * edits (drag-and-drop, gate insertion, qubit-line edits, etc.).
   * The remap is computed by object identity against a snapshot
   * taken at the previous render, so unmoved ops keep their key
   * even when their string location number would otherwise drift.
   *
   * Idempotent against a fixed-point remap (a remap whose new keys
   * map to themselves on the next call). Collisions (two old keys
   * mapping to the same new key) are not expected from Sqore's
   * caller; if they happen, later writes overwrite earlier ones.
   */
  rebase(remap: ReadonlyMap<string, string | null>): void {
    // Build the rebased map in a fresh container, then swap it in.
    // Doing it in two passes (read-only iteration over the current
    // entries, then atomic replacement) is what makes key chains
    // (`a → b`, `b → c`) and key swaps (`a → b`, `b → a`) behave
    // correctly — an in-place rekey would clobber an entry mid-walk
    // before its own rename had a chance to run.
    const next = new Map<string, boolean>();
    for (const [oldKey, value] of this.expanded) {
      if (!remap.has(oldKey)) {
        // Untracked: keep at the same key.
        next.set(oldKey, value);
        continue;
      }
      const newKey = remap.get(oldKey);
      if (newKey == null) continue; // op is gone; drop the entry
      next.set(newKey, value);
    }
    this.expanded.clear();
    for (const [k, v] of next) this.expanded.set(k, v);
  }

  /**
   * Apply user expansion overrides to a freshly-rendered component
   * grid. Walks `grid` recursively and, for any op whose location
   * string has a `ViewState.expanded` entry, sets
   * `dataAttributes.expanded` to `"true"` / `"false"` accordingly.
   *
   * Should be called AFTER the per-render default-expansion passes
   * (`expandOperationsToDepth`, `expandIfSingleOperation`) so user
   * overrides win.
   *
   * @param grid The grid to mutate. Must already have
   *             `dataAttributes.location` populated on every op
   *             (i.e. `fillGateRegistry` has run).
   */
  applyTo(grid: ComponentGrid): void {
    grid.forEach((col) =>
      col.components.forEach((op) => {
        const loc = op.dataAttributes?.["location"];
        if (loc != null) {
          const userPref = this.expanded.get(loc);
          if (userPref !== undefined) {
            if (op.dataAttributes == null) op.dataAttributes = {};
            op.dataAttributes["expanded"] = userPref ? "true" : "false";
          }
        }
        if (op.children != null) {
          this.applyTo(op.children);
        }
      }),
    );
  }
}
