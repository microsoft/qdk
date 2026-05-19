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

  /**
   * `true` if this location comes strictly *before* `other` in
   * document order — i.e. the renderer would visit this op before
   * `other` during a depth-first walk of the component grid.
   *
   * Document-order comparison rules, applied segment-by-segment:
   *
   *   1. Compare `(col, op)` lexicographically (column first, then
   *      opIndex within the column). The smaller pair comes first.
   *   2. If every segment compared so far is equal and one location
   *      ran out of segments, the **shorter** location comes first
   *      — an ancestor renders before its descendants. (E.g. the
   *      group at `"0,0"` renders before its child at `"0,0-0,1"`.)
   *
   * Equal locations return `false` (strict before).
   *
   * Not what you want for "producer must precede consumer" —
   * different ops in the same column are simultaneous, not
   * "before" each other. Use [`inEarlierColumnThan`](#) for that.
   */
  before(other: Location): boolean {
    const n = Math.min(this.segments.length, other.segments.length);
    for (let i = 0; i < n; i++) {
      const [ac, ao] = this.segments[i];
      const [bc, bo] = other.segments[i];
      if (ac !== bc) return ac < bc;
      if (ao !== bo) return ao < bo;
    }
    return this.segments.length < other.segments.length;
  }

  /**
   * `true` if this location is in a strictly **earlier column**
   * than `other` — i.e. the renderer guarantees an op at this
   * location finishes before an op at `other` *starts*, in real
   * time-step order, with ancestor groups projecting their column
   * down onto everything they contain.
   *
   * Used to enforce "producer measurement must finish before its
   * classical consumer starts" for the dropzone filter and the
   * `moveOperation` safety net. The renderer-document-order
   * comparator [`before`](#) is the wrong thing for this: two ops
   * in the same column are simultaneous, not before/after each
   * other, and a consumer "promoted" to the producer's outer
   * column is still in that column even if it's a sibling op.
   *
   * Algorithm, applied segment-by-segment from the root down:
   *
   *   1. At each shared segment index `i`, look at the **column**
   *      indices `(this.col[i], other.col[i])`:
   *      - this.col < other.col → strictly earlier column. Done.
   *      - this.col > other.col → strictly later column. Not earlier.
   *      - equal columns → same time-step at this nesting level;
   *        keep checking deeper.
   *   2. When columns are equal at level `i` but the **op-index**
   *      differs, the two locations are in different ops within
   *      the same column — i.e. siblings at the same time-step,
   *      not predecessor/successor. Not earlier.
   *   3. If every shared segment is fully equal and one location
   *      ran out of segments first, one is an ancestor of the
   *      other (or they're identical). The ancestor "occupies"
   *      the same column as its descendants at every shared
   *      level — not strictly earlier. Not earlier.
   *
   * Worked example. Producer measurement at
   * `"0,0-1,0-0,0-1,0"` (deeply nested inside a `for` at
   * top-level col 0):
   *
   *   - vs. consumer at `"5,X"` (any X) → producer.col[0]=0 < 5 → ✓ earlier.
   *   - vs. consumer at `"0,1"` (top-level col 0, sibling op) →
   *     producer.col[0]=0 == 0, op-indices differ → ✗ same col.
   *   - vs. consumer at `"0,0-2,0"` (same outer group, later
   *     inner col) → equal at i=0, producer.col[1]=1 < 2 → ✓ earlier.
   *   - vs. consumer at `"0,0-1,1"` (same outer + inner col,
   *     different op) → equal at i=0, equal cols at i=1, op-indices
   *     differ → ✗ same col within group.
   */
  inEarlierColumnThan(other: Location): boolean {
    const n = Math.min(this.segments.length, other.segments.length);
    for (let i = 0; i < n; i++) {
      const [ac, ao] = this.segments[i];
      const [bc, bo] = other.segments[i];
      if (ac < bc) return true;
      if (ac > bc) return false;
      // Same column at this level. If the op-indices differ, the
      // two locations branch into different ops here — they're at
      // the same time-step, just on different sibling subtrees.
      if (ao !== bo) return false;
      // Same (col, op) — keep walking; the locations share this
      // segment of the path.
    }
    // Every shared segment was identical and one (or both)
    // location(s) ran out. Ancestor-vs-descendant or equal — both
    // mean "same column at every shared level".
    return false;
  }
}
