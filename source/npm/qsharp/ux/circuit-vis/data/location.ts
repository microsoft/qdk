// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

/**
 * `Location` — value type for hierarchical addresses inside a
 * circuit's `componentGrid`.
 *
 * An operation's address is a string of the form `"col,op"` (top
 * level) or `"col,op-col,op-..."` (nested inside expanded groups).
 * This module owns the parse/compose of that format. The string form
 * is what's stored on the wire (SVG `data-location` /
 * `data-dropzone-location` attributes, `Operation.dataAttributes
 * .location`, `LayoutMap.scopes` keys); `Location` is the in-memory
 * representation callers operate on.
 *
 * Immutable: every "mutation" returns a new `Location` and the
 * underlying `_segments` array is frozen.
 *
 * The empty location is the root scope: `Location.root()` (and
 * `Location.parse("")`) represents the top-level grid, with string
 * form `""`.
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
   * Parse a location string into a `Location`:
   *
   *   - `""` → root (no segments).
   *   - `"0,1"` → one segment.
   *   - `"0,1-2,3"` → two segments.
   *
   * Throws "Invalid location" for any segment that isn't exactly
   * `<int>,<int>`.
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
   * caller already has the numeric tuples (e.g. sqore building child
   * locations during render).
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
   * The location of the scope that *contains* this op. Drops the last
   * segment; `.parent()` on root returns root (so chained walks
   * terminate cleanly). For an op at `"0,1-2,3"`, the parent is
   * `"0,1"`.
   */
  parent(): Location {
    if (this.segments.length <= 1) return Location._ROOT;
    return new Location(Object.freeze(this.segments.slice(0, -1)));
  }

  /**
   * Append a `(col, op)` segment, producing the location of a child
   * inside *this* scope.
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
   * document order — the order the renderer visits ops in a
   * depth-first walk of the grid.
   *
   * Segment-by-segment: compare `(col, op)` lexicographically; if all
   * compared segments are equal, the shorter location (an ancestor)
   * comes first. Equal locations return `false`.
   *
   * Not the same as "producer must precede consumer": ops in the same
   * column are simultaneous, not before/after. Use
   * [`inEarlierColumnThan`](#) for that.
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
   * `true` if this location is in a strictly **earlier column** than
   * `other` — i.e. an op here is guaranteed to finish before an op at
   * `other` starts, in real time-step order, with ancestor groups
   * projecting their column onto everything they contain.
   *
   * Used to enforce "producer measurement must finish before its
   * classical consumer starts" in the dropzone filter and the
   * `moveOperation` safety net. [`before`](#) is wrong for this: two
   * ops in the same column are simultaneous, and a consumer promoted
   * to the producer's outer column shares that column even as a
   * sibling.
   *
   * Walks segment-by-segment from the root: at each shared level, an
   * earlier column wins immediately, a later column loses, and equal
   * columns recurse deeper. Equal columns with differing op-indices
   * are siblings at the same time-step (not earlier); an
   * ancestor-vs-descendant pair shares the column at every level (not
   * earlier).
   */
  inEarlierColumnThan(other: Location): boolean {
    const n = Math.min(this.segments.length, other.segments.length);
    for (let i = 0; i < n; i++) {
      const [ac, ao] = this.segments[i];
      const [bc, bo] = other.segments[i];
      if (ac < bc) return true;
      if (ac > bc) return false;
      // Same column; differing op-indices mean sibling subtrees at
      // the same time-step.
      if (ao !== bo) return false;
      // Same (col, op) — keep walking.
    }
    // One location ran out: ancestor-vs-descendant or equal, both
    // meaning "same column at every shared level".
    return false;
  }
}
