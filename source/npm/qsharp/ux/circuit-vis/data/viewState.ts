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
 * # Position stability (known limitation)
 *
 * Entries are keyed by the op's hierarchical location string
 * (e.g. `"0,0-1,2"`). When an edit shifts an op's position, its
 * `ViewState` entry stays at the old key and silently goes stale.
 * In practice this means: collapsing a group, then inserting a
 * sibling above it, loses the collapse. The right long-term fix is
 * stable IDs (R4's `Location` value type set up the centralization
 * needed for this); for now the simpler keying is good enough and
 * fixes the high-impact bug (every editor mutation losing every
 * expand state).
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
