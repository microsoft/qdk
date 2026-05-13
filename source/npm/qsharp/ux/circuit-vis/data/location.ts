// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

/**
 * `Location` — value type for hierarchical addresses inside a circuit's
 * `componentGrid`.
 *
 * The editor identifies every operation by a hierarchical "location"
 * string of the form `"col,op"` (top level) or `"col,op-col,op-..."`
 * (nested inside expanded groups). This module owns the parse and
 * compose of that format, so the addressing convention lives in
 * exactly one place.
 *
 * String form is preserved on the wire (SVG `data-location` /
 * `data-dropzone-location` attributes, `Operation.dataAttributes
 * .location`, and the `LayoutMap.scopes` keys) — `Location` is the
 * in-memory representation that callers operate on.
 *
 * **Immutable.** Every "mutation" returns a new `Location`; the
 * underlying `_segments` array is frozen. Mirrors the way `Date` and
 * `URL` value types feel in modern TS code.
 *
 * **Empty location = root scope.** `Location.root()` represents the
 * top-level grid, and `Location.parse("")` returns it. Its string
 * form is `""`, which matches the existing `LayoutMap` convention
 * for the top-level scope key.
 */
export class Location {
  /**
   * Frozen segments. Each tuple is `[colIndex, opIndex]` for one
   * level of nesting, ordered root-to-leaf.
   */
  readonly segments: ReadonlyArray<readonly [number, number]>;

  /**
   * Cached root singleton. Shared because `Location` is immutable
   * and the empty case is hit by every top-level dropzone /
   * `parent()` chain that bottoms out.
   */
  private static readonly _ROOT = new Location(Object.freeze([]));

  /** Use one of the static factories. */
  private constructor(segments: ReadonlyArray<readonly [number, number]>) {
    this.segments = segments;
  }

  /** The empty location — addresses the top-level scope itself. */
  static root(): Location {
    return Location._ROOT;
  }

  /**
   * Parse a location string into a `Location`. Mirrors the historical
   * `locationStringToIndexes` semantics:
   *
   *   - `""` → root (no segments).
   *   - `"0,1"` → one segment.
   *   - `"0,1-2,3"` → two segments.
   *
   * Throws on malformed input — same contract the previous helper
   * had ("Invalid location" for any segment that isn't exactly
   * `<int>,<int>`).
   */
  static parse(s: string): Location {
    if (s === "") return Location._ROOT;
    const segments = s.split("-").map((segment): readonly [number, number] => {
      const coords = segment.split(",");
      if (coords.length !== 2) {
        throw new Error("Invalid location");
      }
      const col = parseInt(coords[0]);
      const op = parseInt(coords[1]);
      if (!Number.isInteger(col) || !Number.isInteger(op)) {
        throw new Error("Invalid location");
      }
      return Object.freeze<[number, number]>([col, op]);
    });
    return new Location(Object.freeze(segments));
  }

  /**
   * Build a `Location` from already-parsed segments. Useful when the
   * caller already has the numeric tuples (e.g. recursion in
   * [sqore.ts](sqore.ts) building child locations during render).
   */
  static of(...segments: ReadonlyArray<readonly [number, number]>): Location {
    if (segments.length === 0) return Location._ROOT;
    return new Location(
      Object.freeze(
        segments.map((s) => Object.freeze<[number, number]>([s[0], s[1]])),
      ),
    );
  }

  /** `true` if this is the root scope (no segments). */
  get isRoot(): boolean {
    return this.segments.length === 0;
  }

  /** Number of segments — i.e. how deep this location is nested. */
  get depth(): number {
    return this.segments.length;
  }

  /**
   * Last `(colIndex, opIndex)` segment, or `null` if this is the
   * root location. Most callers that ask for `.last()` are about to
   * dereference into the parent scope's grid at that `(col, op)`.
   */
  last(): readonly [number, number] | null {
    return this.segments.length === 0
      ? null
      : this.segments[this.segments.length - 1];
  }

  /**
   * The location of the scope that *contains* this op. Drops the
   * last segment; calling `.parent()` on root returns root again
   * (no-op rather than throw, so chained walks terminate cleanly).
   *
   * For an op at `"0,1-2,3"`, the parent is `"0,1"` — the same
   * string the editor uses as the `LayoutMap.scopes` key for the
   * scope this op lives in.
   */
  parent(): Location {
    if (this.segments.length <= 1) return Location._ROOT;
    return new Location(Object.freeze(this.segments.slice(0, -1)));
  }

  /**
   * Append a `(col, op)` segment, producing the location of a child
   * inside *this* scope. Used by sqore's recursive
   * `fillGateRegistry` to assign child locations during render, and
   * by the dropzone layer to compose `data-dropzone-location`
   * strings.
   */
  child(col: number, op: number): Location {
    return new Location(
      Object.freeze([
        ...this.segments,
        Object.freeze<[number, number]>([col, op]),
      ]),
    );
  }

  /**
   * Canonical string form. Round-trips with `parse`:
   * `Location.parse(loc.toString()).equals(loc) === true`.
   */
  toString(): string {
    return this.segments.map(([c, o]) => `${c},${o}`).join("-");
  }

  /** Structural equality. */
  equals(other: Location): boolean {
    if (this.segments.length !== other.segments.length) return false;
    for (let i = 0; i < this.segments.length; i++) {
      if (
        this.segments[i][0] !== other.segments[i][0] ||
        this.segments[i][1] !== other.segments[i][1]
      ) {
        return false;
      }
    }
    return true;
  }
}
