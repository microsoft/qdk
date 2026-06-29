// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { ComponentGrid } from "./circuit.js";

/**
 * Per-session view preferences that survive `Sqore.renderCircuit()`
 * but are NOT persisted to the saved circuit (`.qsc`) file.
 *
 * The editor has three state layers with different lifetimes:
 *
 * | Layer              | Lifetime               | Persisted? | Owner              |
 * | ------------------ | ---------------------- | ---------- | ------------------ |
 * | `CircuitModel`     | The circuit's lifetime | Yes (.qsc) | Action layer       |
 * | `ViewState`        | The editor session     | No         | View layer (Sqore) |
 * | `InteractionState` | A single gesture       | No         | Action layer       |
 *
 * `ViewState` holds state the user expects to stay stable while
 * editing but that doesn't belong in the file — chiefly per-group
 * expand/collapse.
 *
 * # Override semantics
 *
 * Only explicit user choices are stored. Default expansion is computed
 * per-render in [sqore.ts](../sqore.ts); user overrides are applied
 * after via [`applyTo`](#method-applyTo):
 *
 *   - Absent entry → defaults win.
 *   - `true` entry → expanded, even if the default would collapse.
 *   - `false` entry → collapsed, even if the default would expand.
 *
 * # Position stability
 *
 * Entries are keyed by the op's location string (e.g. `"0,0-1,2"`),
 * which is not stable under edits that splice columns or grids. The
 * View layer (`Sqore`) snapshots an `op → location` map each render
 * and calls [`rebase`](#method-rebase) on the next render to migrate
 * keys forward by object identity. External tree replacement
 * (`Sqore.updateCircuit`) breaks that identity link, so the snapshot
 * is dropped and the next render starts fresh.
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
   * Collapsing also clears explicit overrides on descendants of
   * `location`, so re-expanding later doesn't auto-spring
   * previously-expanded children back open.
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
   *   - `remap.get(oldKey) === <string>` → rekey to that string.
   *   - `remap.get(oldKey) === null` → drop the entry (op is no
   *     longer in the grid).
   *   - `remap.has(oldKey) === false` → leave unchanged (no info
   *     about this op; keep rather than guess).
   *
   * The View layer ([sqore.ts](../sqore.ts)) calls this each render to
   * track ops whose locations shifted due to upstream edits. The
   * remap is computed by object identity against the previous
   * render's snapshot, so unmoved ops keep their key even when their
   * string location would otherwise drift. Idempotent against a
   * fixed-point remap; on key collisions, later writes win.
   */
  rebase(remap: ReadonlyMap<string, string | null>): void {
    // Build the rebased map in a fresh container, then swap it in.
    // Two passes (read-only iteration, then atomic replacement) is
    // what makes key chains (`a → b`, `b → c`) and key swaps
    // (`a → b`, `b → a`) correct — an in-place rekey would clobber
    // an entry before its own rename had a chance to run.
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
   * Call AFTER the per-render default-expansion passes
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
