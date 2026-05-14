# Circuit Editor — Roadmap & TODO

Living scratchpad for the Circuit Editor (CE) work in progress. Not
exhaustive; captures the items we have lined up but haven't started, plus
the rationale for each so future sessions can pick up without losing
context.

---

## Recently shipped (live-preview arc)

For context — the foundation these next items build on:

- Recursive Q# emission for nested structural groups (`loop:`, `if:`,
  `<scope>`, iteration markers) in
  [circuit_to_qsharp.rs](../../../compiler/qsc_circuit/src/circuit_to_qsharp.rs).
- Live Q# preview pipeline (`qsharp-circuit-preview` URI scheme,
  lazy regeneration on first load) in
  [circuitPreview.ts](../../../../vscode/src/circuitPreview.ts).
- Trace-divergence banner (divergent loop iterations, opaque
  conditionals).
- "Save as Circuit (.qsc)" bridge from the Show-Circuit panel
  ([circuit.ts](../../../../vscode/src/circuit.ts)).
- Custom-gate extraction with transitive closure and
  measurement-aware return types.
- Top-level entry-point wrapper unwrap (one-shot, never recursive).
- Filename-to-identifier sanitization at the Rust entry point and
  mirrored in TypeScript.
- Custom-gate call-site array-wrap convention
  (`Foo([qs[0], qs[1]])` to match `(qs : Qubit[])` signatures).

---

## In progress

Capture work that is actively being designed or implemented, even if
nothing has landed yet.

### Architecture refactor — prerequisite for further editor work

**Why now.** Phase A (in the drag-and-drop section below) shipped
passing tests but the dropzones don't actually work in the running
VS Code editor. The tests asserted _structure_ (which
`data-dropzone-location` attributes are present on the rendered SVG)
but not _geometry_ (whether dropzone rectangles are positioned where
users can hit them). The geometry is wrong because the column-x math
reverse-engineers positions from `data-width` and `x` attributes on
already-rendered host elements, and those are subtly inconsistent for
nested scopes. Patching the math is possible but fragile — the same
shape of bug will surface again as soon as we touch multi-target
authoring (Phase B), the Inspector (#1), structural-group authoring
(#4), or anything else that needs to know "where on screen does op X
live."

The drag-and-drop state machine was the headline issue when we wrote
Phase C, but Phase A surfaced a deeper structural problem: **layout,
model, and editor are tangled via DOM-attribute side channels.**
That's the root cause to address before any non-trivial editor
feature can land cleanly.

#### Source-of-truth findings

These are the architectural pain points discovered while reading
[sqore.ts](sqore.ts), [events.ts](editor/events.ts), [draggable.ts](editor/draggable.ts),
[process.ts](renderer/process.ts), [circuitActions.ts](actions/circuitActions.ts)
(formerly `circuitManipulation.ts`, renamed/restructured in R3),
and [utils.ts](utils.ts). Re-verify before changing any of them.

1. **Geometry is computed twice and recovered approximately.**
   ~~[`processOperations`](renderer/process.ts) computes every column offset
   and gate bounding box correctly, then discards them into SVG
   attributes (`x`, `data-width`, `data-wire-ys`).
   `getColumnOffsetsAndWidths` and [`getWireData`](utils.ts) re-derive
   the same numbers by querying the DOM. The Phase A bug lives in
   this gap.~~ **Resolved in R1+R2:** `processOperations` now exports
   a [`LayoutMap`](renderer/layoutMap.ts) consumed directly by the editor;
   `getColumnOffsetsAndWidths` is deleted. `getWireData` is the only
   remaining DOM-recovery function and is currently kept because it
   has to read the ghost-qubit wire (which is added to the SVG after
   layout); to be addressed in R6.
2. **~~`CircuitEvents` is a god class~~** ✅ Resolved in R5 (with
   prior assists from R3 + R3.5). Was ~700 lines, 11 mutable
   fields, 25+ methods mixing drag state, selection, dropzones,
   qubit reordering, control add/remove, context menu hooks,
   document listeners, ghost elements, and scroll behavior. R3
   moved data fields to [CircuitModel](data/circuitModel.ts); R3.5
   moved ephemeral session-state fields to
   [InteractionState](actions/interactionState.ts) +
   [InteractionActions](actions/interactionActions.ts); R5 carved the
   remaining event-listener wiring into focused controllers
   ([dragController.ts](editor/dragController.ts),
   [selectionController.ts](editor/selectionController.ts),
   [qubitController.ts](editor/qubitController.ts),
   [keyboardController.ts](editor/keyboardController.ts),
   [scrollController.ts](editor/scrollController.ts)) sharing an
   [InteractionContext](editor/interactionContext.ts). `CircuitEvents`
   is now a ~150-line coordinator: build the context,
   instantiate controllers, dispose on teardown.
3. ~~**No pure model layer.** Every function in
   [circuitManipulation.ts](circuitManipulation.ts) takes a
   `CircuitEvents` even though it only needs `componentGrid` /
   `qubits` / `qubitUseCounts`. To unit-test `addOperation` you have
   to construct a fake `CircuitEvents`. JSDOM is required where it
   shouldn't be.~~ **Resolved in R3:** [circuitModel.ts](data/circuitModel.ts)
   owns `componentGrid`, `qubits`, `qubitUseCounts` and their
   invariants. [circuitActions.ts](actions/circuitActions.ts) (replacing the
   old `circuitManipulation.ts`) takes a `CircuitModel` directly;
   functions are unit-testable without JSDOM. See
   [test/circuitActions.test.mjs](../../test/circuitActions.test.mjs)
   for the proof.
4. **Two parallel circuits.** [`Sqore.circuit`](sqore.ts) (original)
   and `_circuit` (deep-copied per-render). The renderer mutates the
   copy (sets `dataAttributes.location`, `expanded`); the editor
   mutates the original. They communicate only by structural identity
   of array indices, which is why hierarchical location strings work
   at all. Brittle.
5. **~~Hierarchical locations are stringly-typed.~~** ✅ Resolved
   in R4. `"0,1-2,3"` was parsed via `location.split("-")` in 8+
   places, composed via template strings in another half-dozen.
   Now centralized in [location.ts](data/location.ts) (`Location` value
   type); the wire format is unchanged but every parse/compose
   goes through one type, opening the door to richer addressing
   schemes (e.g. stable IDs that survive insertions) without
   chasing every spelling.
6. ~~**Editor chrome is appended directly into `svg.qviz`.**~~
   ✅ Resolved in R6. All editor-only DOM (dropzones, ghost qubit
   layer, wire dropzones spawned during drag) now lives inside a
   single `<g class="editor-overlay">` group attached as the last
   child of `svg.qviz`. Renderer-owned children of `svg.qviz`
   (gates, wires, register labels) stay purely presentational, so
   future overlay features (selection rectangles, hover halos,
   Inspector anchors) drop in alongside the existing layers
   without polluting the renderer's output.

#### Goal — three-layer architecture

Separate the concerns currently merged in [sqore.ts](sqore.ts) /
[events.ts](editor/events.ts) / [draggable.ts](editor/draggable.ts) into three
explicit layers. This is a **Model + Actions + View** split — the
same shape as MVC + Command pattern, or the View / Use-cases /
Entities split from Clean Architecture, or the dispatcher / store /
view triplet from Flux. Pick whichever frame is most familiar; they
all describe the same separation.

1. **Data layer.** The persistent circuit definition.
   - `CircuitModel` — owns `componentGrid`, `qubits`,
     `qubitUseCounts`. Plain data + invariant maintenance only.
   - `Location` value type — addresses nodes in the data; replaces
     today's stringly-typed `"0,1-2,3"` parsing scattered across
     8+ files.
   - **No DOM. No interaction state.** Fully unit-testable without
     JSDOM.
2. **Action layer (business logic).** A narrow, well-named API of
   mutations triggerable by either UI or programmatic callers
   (including tests).
   - `CircuitActions` — pure functions taking `(model, ...args)`:
     `addOperation`, `moveOperation`, `removeOperation`,
     `addControl`, `removeControl`, `moveQubit`, `removeQubit`,
     etc. Today's [circuitManipulation.ts](circuitManipulation.ts)
     with the `CircuitEvents` dependency removed.
   - `InteractionState` — ephemeral session state distinct from
     the saved circuit: `selectedLocation`, `dragInProgress`,
     `pickingControl`, etc. Today's loose fields on `CircuitEvents`.
   - `InteractionActions` — `select(loc)`, `beginDrag(...)`,
     `commitDrag(...)`, `cancelDrag()`, `beginPickingControl(...)`.
     Mutate `InteractionState`, may chain into `CircuitActions`.
3. **View layer.** Rendering and event-to-action translation.
   - `processOperations` + `LayoutMap` — given a `CircuitModel`,
     produce the SVG **and** a queryable geometry map. One pass,
     one source of truth (vs. today's compute-then-recover).
   - Editor overlay — dropzones, ghosts, hover halos, future
     selection rectangles. Rendered into a sibling
     `<g class="editor-overlay">` positioned via `LayoutMap`.
   - **Controllers** — `DragController`, `SelectionController`,
     `QubitController`, `KeyboardController`, `ScrollController`.
     Each owns a slice of pointer/keyboard event wiring. Bodies are
     trivial: translate raw events into `*Actions.*` calls. **No
     business logic.**

The layers compose downward only:
**View → Actions → Data.** Data has no idea the View exists.
Actions have no idea the View exists. View talks to Data only via
`LayoutMap` (read) and `*Actions.*` (write).

#### Resolved design questions

These three knobs each have multiple defensible answers; pinning
them down now prevents drift.

1. **How does the View find out the Data changed?**
   **Imperative re-render.** Action callers call `view.render()`
   after each Action (today's `renderFn()`). No observer / pub-sub.
   Simple, no transaction-boundary hazards, easy to debug.
   Could graduate to a subscription model later if incremental
   rendering becomes worth the complexity, but it isn't now.
2. **Do Actions mutate or return new state?**
   **Mutate, return `void`** (or a small status code if the caller
   needs to know success/failure). No deep-copy ceremony, no
   forced rebuild on every reader. Callers that need the new
   entity look it up by `Location`. Today's mixed
   "mutate-and-also-return-the-new-op" is the smell to fix.
   Trade-off acknowledged: no free undo/redo; revisit if undo lands
   on the roadmap.
3. **Where does ephemeral interaction state live?**
   **Separate `InteractionState`**, not in `CircuitModel`. Saved
   circuit JSON must never accidentally include `selectedLocation`
   or `dragInProgress`. Mutations to `InteractionState` don't
   trigger circuit re-renders (they may trigger overlay updates
   only, once R6 lands).

#### Non-goals

- Not rewriting [`processOperations`](renderer/process.ts) /
  [gateFormatter.ts](renderer/formatters/gateFormatter.ts). They already
  compute the right numbers; we just need to export them.
- Not changing the data shape (`Operation`, `Circuit`,
  `CircuitGroup`).
- Not changing the public surface (`Sqore.draw`, `EditorHandlers`,
  `editCallback`, `getCurrentCircuitModel`). Internal seams change;
  callers don't.
- Not switching to a UI framework (React/Lit/etc.). The renderer
  stays direct DOM/SVG.

#### Phased plan

Same discipline as the drag-and-drop overhaul: each phase is
independently shippable, regression-tested against the existing
12-circuit snapshot suite, and reversible.

##### R1 — `LayoutMap` as a first-class output of `processOperations` ✅ DONE

The single highest-leverage change. Stop discarding the geometry
that the layout pass already computes.

**Status: complete.** All 124 npm tests pass, including 2 new
pixel-coordinate dropzone tests that verify on-column dropzones
overlap their gates' x-ranges (the property whose absence caused
the Phase A bug).

**Delivered:**

1. New [layoutMap.ts](renderer/layoutMap.ts) defines:
   ```ts
   type LayoutScope = { columnXOffsets: number[]; columnWidths: number[] };
   type LayoutMap = { scopes: Map<string, LayoutScope>; wireYs: number[] };
   ```
   Keys are parent-op location strings (`""` = root, `"0,0"` = first
   nested scope, etc.). `columnXOffsets[i]` is the gate's left-edge x
   for column `i` (matches `_fillRenderDataX`'s `colStartX[i]`).
2. [`processOperations`](renderer/process.ts) now also returns
   `localScope` (in startX-anchored local coords) and `childScopes`
   (Map, already absolute). `_fillRenderDataX` returns
   `{endX, colStartX, childScopes}`; for each Group it shifts the
   recursive call's `localScope.columnXOffsets` by the appropriate
   `offset` (= `groupLeftX - startX + groupPaddingX [+ controlCircleOffset]`)
   and merges into childScopes keyed by the parent op's location.
   `_processChildren` stashes the recursive layout info on
   `renderData._childLayout` (internal field on
   [GateRenderData](renderer/gateRenderData.ts)).
3. [`Sqore.compose`](sqore.ts) builds the LayoutMap and surfaces it
   on `ComposedSqore`. `renderCircuit` plumbs it to
   `createDropzones(container, sqore, layoutMap)` and
   `enableEvents(container, sqore, layoutMap, useRefresh)`.
4. [`draggable.ts`](editor/draggable.ts) now sources column geometry from
   `layoutMap.scopes.get(pathPrefix)` via a `scopeToColArray` bridge.
   No more reverse-engineering from rendered SVG attributes for
   dropzone placement — the source of the Phase A bug is gone.
5. [`events.ts`](editor/events.ts) builds `columnXData` from the LayoutMap
   instead of `getColumnOffsetsAndWidths`.
6. The existing `data-location` / `data-wire-ys` / `data-width` SVG
   attributes are kept — they remain the canonical link from a
   rendered SVG element back to its `Operation`. They are now
   denormalized views, not the source of truth for layout.

**Bridge convention (resolved in R2).** R1 introduced a transitional
`-gatePadding` shift in `scopeToColArray` and `events.ts`'s
`columnXData` because `makeDropzoneBox` was calibrated to a
`xOffset = colStartX - gatePadding` convention. Both bridges and
the convention itself were removed in R2 — `makeDropzoneBox` now
takes a `LayoutScope` directly and uses `colStartX` natively.

**Tests added:**

- `flat circuit: every gate is covered by its on-column dropzone`
  — for a 3-column flat circuit, every rendered gate's x-range is
  overlapped by an on-column dropzone in its column.
- `expanded group: nested gates are covered by their on-column dropzones`
  — same property for gates inside an expanded group. **This is
  the test that would have caught the original Phase A bug.**

Both tests parse hierarchical location strings into
`(scope-prefix, colIndex, opIndex)` and assert by _column_ identity
rather than full-location identity (since on-column dropzones in a
column with N ops use opIndices `0..N`, not the same opIndex as
the host gate).

##### R2 — Retry "edit inside groups" with `LayoutMap` (the real Phase A) ✅ DONE

The cleanup pass that closes out Phase A. Status: complete; all 125
npm tests pass (the R1 set plus a new trailing-append-column pixel
test).

**Delivered:**

1. [`makeDropzoneBox`](editor/draggable.ts) now takes a `LayoutScope`
   directly instead of the `{ xOffset, colWidth }[]` bridge. The
   `colStartX - gatePadding` shift convention introduced as a
   transitional bridge in R1 is gone — the function operates on the
   layout pass's native `colStartX` numbers.
2. Two named constants (`INTER_COLUMN_HALF_WIDTH`,
   `DROPZONE_PADDING_Y`) replace the magic `gatePadding * 4` /
   `paddingY * 2` arithmetic, and a small `columnGeometry()` helper
   handles the past-end synthesis (used for the trailing-append
   column) explicitly rather than via out-of-bounds array indexing
   inside `makeDropzoneBox`.
3. `_dropzoneLayer` extracted into `_dropzoneLayer` +
   `_appendTrailingColumn` — the trailing-column case is no longer
   inlined into the recursion-driver function.
4. New `composeLocation(prefix, col, op)` helper centralizes the
   hierarchical-location-string composition (`"col,op"` at top
   level, `"prefix-col,op"` nested). Two call sites now share it.
   _Update: subsumed by R4's `Location` value type; the helper
   was retired and its body now delegates to
   `Location.parse(prefix).child(col, op).toString()` until the
   final remaining call sites in [draggable.ts](editor/draggable.ts)
   thread `Location` through directly._
5. `getColumnOffsetsAndWidths` (the DOM-attribute reverse-engineering
   function that caused the original Phase A bug) is **deleted**.
   So is the `scopeToColArray` bridge from R1, and the now-orphaned
   `findLocation` helper in [utils.ts](utils.ts).
6. [`events.ts`](editor/events.ts) now stores the full `LayoutMap` instead
   of just a top-level `columnXData` array. Per-op temporary
   dropzones (created when dragging a multi-target gate) look up the
   _parent scope_ of the selected op at use time. **This closes a
   latent bug:** previously, a multi-target gate inside an expanded
   group would have its temporary dropzones positioned using
   top-level column geometry, so they'd land at wrong screen
   positions whenever the op was nested.

**Tests added:**

- `trailing-append column lands past the rightmost gate` — locks
  down the past-end synthesis math in `columnGeometry`. Asserts
  the trailing dropzones' centers lie past the rightmost gate's
  right edge, and their left edges don't bleed into the last
  column's body.

##### R3 — Data layer: `CircuitModel` + Action layer: `CircuitActions` ✅ DONE

Status: complete. 132 npm tests pass (the 125 from R2 plus 7 new
direct-on-`CircuitModel` tests in
[test/circuitActions.test.mjs](../../test/circuitActions.test.mjs)).

**Delivered:**

1. New [circuitModel.ts](data/circuitModel.ts) defines `CircuitModel`
   (Data layer entity). Owns `componentGrid`, `qubits`,
   `qubitUseCounts`, plus invariant maintenance:
   `ensureQubitCount`, `removeTrailingUnusedQubits`,
   `incrementQubitUseCountForOp`, `decrementQubitUseCountForOp`,
   `snapshot`. The constructor borrows `componentGrid` and
   `qubits` from the input `Circuit` by reference — intentional
   aliasing, so the renderer's `Sqore` and the editor see the same
   data. Derives `qubitUseCounts` by walking the initial grid.
2. [circuitManipulation.ts](circuitManipulation.ts) is **deleted**
   and replaced by [circuitActions.ts](actions/circuitActions.ts) (Action
   layer). Every exported function takes a `CircuitModel` as its
   first argument and mutates it in place — no `CircuitEvents`
   dependency. **No DOM, no interaction state, no rendering.**
3. Two new actions extracted from the old inline logic in
   `events.ts`:
   - `moveQubit(model, src, dst, isBetween)` — replaces the ~70
     line `qubitDropzoneMouseupHandler` body. The handler is now
     a 2-line shell: `moveQubit(...)` + `renderFn()`.
   - `removeQubit(model, qubitIdx)` — replaces the wire-removal
     half of `removeQubitLineWithConfirmation`. The
     window.confirm prompt + `findAndRemoveOperations` orchestration
     stays on `CircuitEvents` until R3.5.
4. `CircuitEvents` now holds a `readonly model: CircuitModel`
   field. `componentGrid` / `qubits` / `qubitUseCounts` survive
   as getter delegations to keep `getCurrentCircuitModel()` and
   ~25 internal call sites working without churn. The redundant
   `incrementQubitUseCountForOp` / `decrementQubitUseCountForOp`
   methods on `CircuitEvents` are deleted (callers go through
   `this.model.*` via the actions now).
5. [contextMenu.ts](editor/contextMenu.ts) imports from
   `./circuitActions.js` and passes `circuitEvents.model` to
   `removeOperation` / `removeControl`.
6. Public API surface unchanged. `getCurrentCircuitModel(svg)`
   still returns `{ qubits, componentGrid }`; the state-viz
   bridge is unaffected.

**Tests added:**
[test/circuitActions.test.mjs](../../test/circuitActions.test.mjs)
exercises the Action layer directly against a freshly-constructed
`CircuitModel` — **no JSDOM, no `CircuitEvents` stub**. Coverage:

- `CircuitModel` constructor seeds `qubitUseCounts` from the
  initial grid (1 control + 1 target → counts `[1, 1, 0]`).
- `addOperation` appends to the target column and bumps
  `qubitUseCounts`; locks down that the returned op is the
  inserted reference (not a defensive copy of the stored op).
- `removeOperation` drops the op, decrements counts, and
  triggers the trailing-wire trim.
- `addControl` / `removeControl` maintain `qubitUseCounts`,
  grow `qubits` when adding a control on a new wire, and shrink
  it when removing one on the trailing wire. No-op on
  duplicate-add.
- `findAndRemoveOperations` decrements counts and prunes empty
  columns, **but does NOT trim trailing wires** (callers
  decide).
- `moveQubit` swaps register references and re-sorts each
  column by lowest-numbered register; renumbers qubit ids.
- `removeQubit` shifts higher wire indices down by one and
  renumbers qubit ids.

That direct testability is the R3 win — the file would have been
unwritable against the pre-R3 `circuitManipulation.ts` API
without spinning up JSDOM and a fake `CircuitEvents`.

**Deferred to R3.5/R5:**

- ~~`CircuitEvents` still has the loose interaction-state fields
  (`selectedOperation`, `selectedWire`, `dragging`, etc.). R3.5
  carves these into `InteractionState`.~~ **Resolved in R3.5:**
  see [interactionState.ts](actions/interactionState.ts) +
  [interactionActions.ts](actions/interactionActions.ts).
- `CircuitModel` does not yet expose `findOperation` /
  `findParentArray` as methods; the actions still call the
  module-level helpers in [utils.ts](utils.ts). These get
  pulled onto the model when R4's `Location` value type lands.

##### R3.5 — Action layer: `InteractionState` + `InteractionActions` ✅ DONE

Status: complete. 142 npm tests pass (the 132 from R3 plus 10 new
direct-on-`InteractionState` tests in
[test/interactionActions.test.mjs](../../test/interactionActions.test.mjs)).

**Delivered:**

1. New [interactionState.ts](actions/interactionState.ts) defines
   `InteractionState` — the ephemeral session-state container
   (Action layer's state). Owns the seven loose fields that
   previously sat directly on `CircuitEvents`:
   `selectedOperation`, `selectedWire`, `movingControl`,
   `mouseUpOnCircuit`, `dragging`, `disableLeftAutoScroll`,
   `temporaryDropzones`. Pure data — no methods, no DOM
   constructors. Documents the **persistent vs. transient**
   distinction (selectedOperation survives `resetTransient`;
   everything else doesn't) so the next person to touch this
   doesn't have to rediscover it from the call sites.
2. New [interactionActions.ts](actions/interactionActions.ts) defines the
   matching Action layer. Mirrors the shape of
   [circuitActions.ts](actions/circuitActions.ts): each function takes an
   `InteractionState` as first arg and mutates it. Exports:
   - `resetTransient` — replaces the multi-line `_resetState` body.
   - `clearSelection` — drops `selectedOperation`.
   - `markSelected` / `markMovingControl` / `markMouseUpOnCircuit` /
     `markDragging` — single-field setters, used where intent
     beats one-liners (and for test coverage of the contract).
   - `beginToolboxDrag` — sets `selectedOperation` AND
     `disableLeftAutoScroll` together (forgetting the latter
     produces a runaway-scroll bug while the cursor is still over
     the toolbox panel).
   - `trackTemporaryDropzone` — append to the tracked overlay list.
   - `clearTemporaryDropzones` — DOM-touching teardown of tracked
     overlays. The only function in this module that touches the
     DOM; pure-data tests don't exercise it.
3. `CircuitEvents` now holds a `readonly interaction: InteractionState`
   field (next to its `readonly model: CircuitModel`). The seven
   loose fields are gone; every call site reads/writes through
   `this.interaction.*`. The original semantics are preserved
   exactly — `_resetState()` is now a one-line shell that calls
   `resetTransient(this.interaction)`; the toolbox `mousedown`
   handler calls `beginToolboxDrag` instead of setting two fields
   inline; per-op and qubit-line dropzone tracking goes through
   `trackTemporaryDropzone`.
4. No public API surface change. `CircuitEvents.interaction` is
   readable but not exported (the `CircuitEvents` class itself
   stays internal); `getCurrentCircuitModel` still returns
   `{ qubits, componentGrid }` from the model.

**Tests added:**
[test/interactionActions.test.mjs](../../test/interactionActions.test.mjs)
exercises the Action layer directly against fresh `InteractionState`
instances. **No JSDOM** for the pure-data helpers; one tiny
hand-rolled stub `parentNode` is enough for the DOM-touching
`clearTemporaryDropzones`. Coverage:

- `InteractionState` defaults — every field starts in a "no
  gesture" state.
- `resetTransient` clears every transient flag but **preserves**
  `selectedOperation` (the contract that lets the context menu
  still find its target after a mouseup).
- `clearSelection` drops `selectedOperation` and nothing else.
- `markSelected` accepts an op or `null`.
- `beginToolboxDrag` sets `selectedOperation` AND
  `disableLeftAutoScroll` — the regression-prevention test for
  the runaway-scroll bug.
- `markMovingControl` / `markMouseUpOnCircuit` / `markDragging`
  set their respective flags.
- `trackTemporaryDropzone` appends without disturbing existing
  entries.
- `clearTemporaryDropzones` removes each element from its
  parent and clears the list.
- `clearTemporaryDropzones` tolerates dropzones with no
  `parentNode` (already detached elsewhere).
- `clearTemporaryDropzones` is idempotent.

**Deferred to R5:**

- The controllers (`DragController`, `SelectionController`, etc.)
  are not yet carved out; `CircuitEvents` still owns all the event
  wiring. R3.5 makes the carve-out trivial — controllers will read
  `this.interaction.*` directly when extracted.
- The `drag` / `picking` sub-objects suggested in the original
  R3.5 design (a structured drag-mode discriminator) are NOT
  introduced. The current flat-fields shape preserves the
  existing semantics with zero behavioral risk; the structured
  shape is a nicer surface for the controllers and lands with R5.

##### R4 — Data layer: `Location` value type ✅ DONE

Status: complete. 156 npm tests pass (the 142 from R3.5 plus 14
new direct-on-`Location` tests in
[test/location.test.mjs](../../test/location.test.mjs)).

**Delivered:**

1. New [location.ts](data/location.ts) defines `Location` — an
   immutable value type for hierarchical addresses inside a
   circuit's `componentGrid`. Frozen `segments: ReadonlyArray<
readonly [number, number]>`, `private constructor`, static
   factories `root()` / `parse(s)` / `of(...segments)`. A cached
   `_ROOT` singleton avoids re-allocating empty locations on
   every `parent()` chain that bottoms out.
2. Methods mirror the access patterns the call-site survey
   uncovered: `last()` (returns the deepest `[col, op]` or
   `null`), `parent()` (drops the last segment; root → root, no
   throw), `child(col, op)` (appends), `toString()` (round-trips
   with `parse`), `equals(other)` (structural). Getters `isRoot`
   and `depth` for the common queries.
3. `parse` is **stricter than the helper it replaced.** The old
   `locationStringToIndexes` accepted segments like `"1,"` and
   silently produced `[1, NaN]`; `Location.parse` now throws
   `"Invalid location"` on any non-integer coord. No real input
   triggers this — the only producers are numeric template
   literals — but it shores up the value type.
4. **Wire format unchanged.** SVG `data-location` /
   `data-dropzone-location` attributes, `Operation.dataAttributes
.location`, and `LayoutMap.scopes` keys are still the same
   `"col,op"` / `"col,op-col,op-..."` strings. `Location` only
   centralizes the parse/compose; the editor's externally-visible
   surface didn't change.
5. Migrated **all 7 internal call sites:**
   - [utils.ts](utils.ts): `findParentOperation`,
     `findParentArray`, `findOperation` now consume
     `Location.parse(loc).segments` /
     `Location.parse(loc).parent().segments` /
     `Location.parse(loc).last()`.
     `locationStringToIndexes` is **removed** from the export
     list — it was never imported outside the editor.
   - [circuitActions.ts](actions/circuitActions.ts): `_moveX` and
     `addOperation` use `Location.parse(targetLocation).last()`.
     `_addOp`'s `targetLastIndex` parameter is now
     `readonly [number, number]` to accept the immutable tuple.
   - [draggable.ts](editor/draggable.ts): `composeLocation(prefix, c, o)`
     is unexported and now delegates to
     `Location.parse(prefix).child(c, o).toString()` — one-line
     wrapper kept because the surrounding `_populateDropzonesForGrid`
     recursion still threads a `pathPrefix: string` (the same key
     it uses for `LayoutMap.scopes.get(pathPrefix)`). Threading
     `Location` through that recursion is left for R5/R6 when
     `LayoutMap`'s key type becomes worth touching.
   - [events.ts](editor/events.ts): the awkward
     `selectedLocation.lastIndexOf("-")` parent-prefix hack on
     line ~475 is **gone** — replaced by
     `Location.parse(selectedLocation).parent().toString()`.
     `_startAddingControl`'s post-success bookkeeping uses
     `Location.parse(loc).last()`.
   - [sqore.ts](sqore.ts): `fillGateRegistry` now takes a
     `Location` and recurses via `location.child(colIndex, i)`.
     The template-literal compose `${location}-${colIndex},${i}`
     is gone.

**Tests added:**

[test/location.test.mjs](../../test/location.test.mjs) covers:

- `root()` returns the cached singleton (identity preserved).
- `parse("")` returns root.
- `parse("0,1")` and `parse("0,1-2,3")` produce the right depth
  and `last()`.
- `parse` round-trips through `toString` for representative
  inputs.
- `parse` throws `"Invalid location"` on every malformed shape
  (non-integers, missing segments, trailing/leading dashes,
  doubled separators).
- `parent()` of root returns root (no throw); `parent()` of a
  one-segment location returns root; deeper parents drop the
  last segment.
- `child` appends; `child().parent()` round-trips.
- `equals` is structural, handles same/different lengths and
  same/different values.
- `of(...)` matches `parse(...)`.
- Instances are immutable (frozen segments throw on assignment).

**What this unblocks:**

- **R5** (controller carve-out): controllers can pass `Location`
  values around without re-parsing on every hop, and the
  selection / drag state in [interactionState.ts](actions/interactionState.ts)
  can move from `selectedLocation: string | null` to
  `selectedLocation: Location | null` whenever convenient.
- **R6** (editor overlay): the overlay tree's `data-*` attributes
  can be authored via `Location.toString()` once and the parse
  side has a single home.
- **Future:** stable IDs that survive insertions, named children,
  or any other addressing change — only [location.ts](data/location.ts)
  and `LayoutMap.scopes`'s key type need to move.

##### R5 — View layer: split `CircuitEvents` into focused controllers ✅ DONE

Status: complete. 162 npm tests pass (the 156 from R4 plus 6 new
direct-on-`KeyboardController` tests in
[test/keyboardController.test.mjs](../../test/keyboardController.test.mjs)).

**Delivered:**

1. New [interactionContext.ts](editor/interactionContext.ts) defines
   `InteractionContext` — the shared-deps bundle every controller
   receives at construction. Fields: `model`, `interaction`,
   `layoutMap`, `container`, `circuitSvg`, `dropzoneLayer`,
   `ghostQubitLayer`, `wireData`, `renderFn`. Built once in
   [events.ts](editor/events.ts)'s constructor and handed to each
   controller. `wireData` is mutable on the context object
   because qubit-line removals splice an entry out.
2. New [keyboardController.ts](editor/keyboardController.ts) — owns
   document `keydown` / `keyup` for the Ctrl-toggle that swaps
   `moving` / `copying` CSS classes on the container while a
   placed gate is selected. Has `dispose()` because its
   listeners are document-level. Smallest controller; serves as
   the testability proof.
3. New [selectionController.ts](editor/selectionController.ts) — owns
   mousedown on host elements (control dots, target circles,
   etc.). Sets `selectedWire` and the `movingControl` flag.
   Attaches the context menu via
   [contextMenu.ts](editor/contextMenu.ts)'s
   `addContextMenuToHostElem`. No `dispose()` — host elements
   live inside the SVG, replaced wholesale on each
   `enableEvents` re-run.
4. New [dragController.ts](editor/dragController.ts) — owns the gate
   drag-and-drop surface. Carries the bulk of the carve-out:
   gate-element mousedown (drag start + per-op temp dropzones),
   toolbox mousedown (drag from toolbox), dropzone mouseup
   (commit drop), document mouseup (cancel + drag-out-delete),
   document mousedown (clear wire dropzones), the container /
   circuitSvg mouseup overlay-hide pair, ghost element creation,
   and the wire-pick `startAddingControl` / `startRemovingControl`
   flow that the context menu invokes.
   Holds a `QubitController` reference (constructor injection)
   for the one document-mouseup path that detects a qubit
   drag-off and delegates to `removeQubitLineWithConfirmation`.
   Has `dispose()` because its listeners are document-level.
5. New [qubitController.ts](editor/qubitController.ts) — owns qubit-line
   interactions: mousedown on a qubit-label spawns swap and
   insert-between dropzones; mouseup commits via `moveQubit`.
   Also owns `removeQubitLineWithConfirmation` (used by both
   the context menu — via `CircuitEvents` delegation — and the
   drag controller's drag-out-delete path).
6. New [scrollController.ts](editor/scrollController.ts) — just the
   `enableAutoScroll(circuitSvg, interaction)` function.
   Standalone today already; just lifted out so the gate-drag
   and qubit-drag flows can share it without going through
   `CircuitEvents`.
7. New [prompts.ts](editor/prompts.ts) — the `_createConfirmPrompt`
   helper extracted out of [events.ts](editor/events.ts), because
   `QubitController` needs it for the qubit-line removal
   confirmation. Pure DOM, no editor dependencies.
8. [events.ts](editor/events.ts) — the `CircuitEvents` god class is
   gone, replaced by a ~150-line coordinator. Its job is now:
   build the `CircuitModel`, build the `InteractionContext`,
   instantiate the five controllers in dependency order
   (qubit → drag → keyboard → selection), and chain `dispose()`
   through to the controllers that own document-level listeners
   (drag and keyboard). Backward-compat shims:
   - `componentGrid` / `qubits` / `qubitUseCounts` getters keep
     `getCurrentCircuitModel` and [contextMenu.ts](editor/contextMenu.ts)
     working unchanged.
   - `_startAddingControl` / `_startRemovingControl` delegate to
     `DragController` so the context menu can keep invoking them
     by name. These will go away once `addContextMenuToHostElem`
     itself migrates to a controller-shaped API.

**What this looks like in practice:**

Before R5, adding (say) a Shift-click multi-select would mean
adding another field, another handler installer, another handler,
and another `_resetState` participant to `CircuitEvents`. After R5
it's a new `MultiSelectController` that takes the same
`InteractionContext`, owns its own listeners and `dispose()`, and
is instantiated alongside the others in [events.ts](editor/events.ts).
Total `CircuitEvents` churn: one line.

**Tests added:**

[test/keyboardController.test.mjs](../../test/keyboardController.test.mjs)
proves controllers can be exercised in isolation against a stub
`InteractionContext`. Six tests covering:

- Ctrl-down with no selection is a no-op (CSS classes untouched).
- Ctrl-down on a placed gate flips `moving` → `copying`.
- Ctrl-up flips `copying` → `moving`.
- Non-Ctrl keys are ignored entirely.
- Toolbox-drag (op without a `dataAttributes.location`) is treated
  as no-selection — locks down the
  `getGateLocationString`-based gate-vs-toolbox discrimination.
- `dispose()` removes the document listeners.

The larger controllers (drag, qubit, selection) don't yet have
direct tests — the existing snapshot tests and the dropzone
render tests from R2 cover them indirectly. Direct tests can be
added as needed without touching `CircuitEvents`.

**What this unblocks:**

- **R6** (editor overlay): now that the View layer is split, the
  controllers each know which DOM nodes they own — lifting them
  into a dedicated `<g class="editor-overlay">` is a per-controller
  change rather than a god-class rewrite.
- **#2 Gate Inspector**: a new `InspectorController` (selection
  events → `Inspector` panel state) drops in alongside the
  existing five.
- **PointerEvents migration** (the original R5 design called for
  this): now a per-controller change — swap `mousedown`/`mousemove`/
  `mouseup` for `pointerdown`/`pointermove`/`pointerup` +
  `setPointerCapture` inside `DragController` and
  `QubitController` without touching anyone else. Deferred to a
  follow-up to keep R5's behavior change scope at zero.

**Behavior preserved:** every event flow that worked before R5
works the same after. The carve-out is purely structural — same
listeners on the same elements, same `*Actions.*` calls in the
same order, same DOM mutations. Snapshot tests + the existing
dropzone tests cover this.

##### R6 — View layer: editor overlay ✅ DONE

Move all editor-only DOM into a dedicated
`<g class="editor-overlay">` so `svg.qviz`'s direct children stay
purely presentational.

**Status: complete.** All 163 npm tests pass, including a new
structural test in [test/dropzones.test.mjs](../../test/dropzones.test.mjs)
that asserts exactly one `g.editor-overlay` exists as a direct
child of `svg.qviz`, both `.dropzone-layer` and `.ghost-qubit-layer`
live inside it, and no editor-only layers leak out as siblings.

**Delivered:**

1. [draggable.ts](editor/draggable.ts)'s `createDropzones` now builds the
   overlay first and parents both the ghost-qubit layer and the
   dropzone layer inside it. Returns the overlay `<g>` so callers
   can attach further editor-only DOM without re-querying.
2. `_ghostQubitLayer` is pure-create after R6 — it no longer
   self-attaches via the awkward `svg.querySelector` +
   `insertBefore` dance. The one remaining side effect (extending
   `svg.height` / `viewBox` for the trailing ghost wire) is a
   renderer-side dimension change and stays at the SVG root.
3. [interactionContext.ts](editor/interactionContext.ts) gains a
   `readonly overlayLayer: SVGGElement` field; [events.ts](editor/events.ts)
   resolves it once via `container.querySelector(".editor-overlay")`
   and hands it to every controller in the shared context.
4. The four wire-dropzone spawn sites — two in
   [dragController.ts](editor/dragController.ts) (`startAddingControl`,
   `startRemovingControl`) and two in
   [qubitController.ts](editor/qubitController.ts) (swap and
   insert-between dropzones during qubit-label drag) — now append
   to `ctx.overlayLayer` instead of `ctx.circuitSvg`.
5. The `.qsc` snapshot baselines in
   [test/circuits-cases/](../../test/circuits-cases/) were
   regenerated to capture the new wrapper. The `.qs` baselines
   were unchanged — they don't render the editor branch.

**Behavior preserved:** every wire-dropzone query still works
(`circuitSvg.querySelectorAll(".dropzone-full-wire")` is a
descendant search, unaffected by the new wrapper); the dropzone
commit handler doesn't care which subtree the dropzone lives in;
the drag-end cleanup that hides the dropzone/ghost layers via
`style.display = "none"` still finds them via the resolved
`dropzoneLayer` / `ghostQubitLayer` references in the context.

**What this unblocks:**

- **#2 Gate Inspector**: hover halos and selection rectangles
  drop in as new sub-layers of the overlay; the Inspector panel's
  per-gate anchor lines can be drawn into the overlay too,
  avoiding any geometry duplication with the renderer.
- **Multi-select rectangles**: a marching-ants `<rect>` lives in
  the overlay without touching the rendered gates.
- **Future hit-test debugging**: an opt-in dev mode can paint the
  overlay with semi-transparent fills to visualize dropzones
  without re-rendering the whole circuit.

#### What this unblocks

| Planned item                         | Needs            |
| ------------------------------------ | ---------------- |
| Drag-and-drop Phase B (multi-target) | R1, R3, R3.5     |
| #2 Gate Inspector                    | R3, R3.5, R5, R6 |
| #3 Snapshot tool                     | R3, R5           |
| #4 Custom-gate palette               | R3               |
| #5 Structural-group authoring        | R1, R3, R5       |

R1 + R3 are the prerequisites for almost everything else. R2 is
the freebie that pays back the Phase A debt. R3.5 unblocks every
controller-level work.

#### Working principles

- **Tests-first**, with one addition: **assert pixel coordinates,
  not just structure.** Phase A's tests passed because they only
  asserted "a dropzone with this `data-dropzone-location` exists" —
  they should have asserted "a dropzone with this location exists
  at this `(x, y, width, height)`." R1 lets us write tests that
  way; R2+ tests should follow the new pattern.
- **Phases are independent.** R1, R3, R4 don't depend on each
  other. R2 needs R1; R3.5 builds on R3; R5 builds on R3 + R3.5 + R4;
  R6 builds on R5.
- **Preserve current behavior on every flow we don't intend to
  change.** Snapshot suite is the gate.
- **No drive-by refactors.** This _is_ the refactor. Resist
  cleaning up adjacent code while passing through.

---

### Drag-and-drop overhaul

The current drag-and-drop mechanics are clunky and interact poorly with
group nodes and multi-target gates. The earlier decision to prevent
editing inside groups (loops/conditionals/scopes/custom gates) is too
restrictive — users need to be able to author and edit the body of a
group as easily as the top-level grid.

Detailed plan below — captured here because this is a long, careful
task with **no existing unit-test coverage on the drag/drop surface**
and it's easy to silently break user-flows. Each phase should land
with tests before moving to the next.

#### Source-of-truth findings (read these before changing anything)

These are the load-bearing observations from a careful read-through of
[draggable.ts](editor/draggable.ts), [events.ts](editor/events.ts), and
[sqore.ts](sqore.ts). They explain _why_ the editor behaves the way
it does today, and they should be re-verified before any change to
the relevant region.

1. ~~**Dropzones don't recurse into expanded groups.**
   [`_dropzoneLayer`](editor/draggable.ts) iterates only
   `sqore.circuit.componentGrid` — the top-level grid. Group children
   render visually (with the dashed border in `isExpandedGroup`,
   [sqore.ts](sqore.ts)) but no dropzones are generated inside them.~~
   **Resolved in R1+R2:** `_dropzoneLayer` recurses through
   `LayoutMap.scopes`; expanded groups get nested dropzones with
   hierarchical location strings, and they actually land at the
   right screen positions.
2. ~~**Column geometry is top-level only.**
   [`getColumnOffsetsAndWidths`](editor/draggable.ts) explicitly filters
   `indexes.length != 1`, so it can't position dropzones for nested
   columns.~~ **Resolved in R1+R2:** `LayoutMap` exposes per-scope
   geometry; `getColumnOffsetsAndWidths` is deleted.
3. **The data model already supports nested editing end-to-end.**
   Location strings are hierarchical (`"0,1-2,3"` joined by `-`), and
   [`findOperation`](utils.ts) / [`findParentArray`](utils.ts) already
   navigate them. The `addOperation` / `moveOperation` calls in the
   `dropzoneMouseupHandler` already use `findParentArray`, so they
   should work for nested locations once dropzones expose them.
4. **State management is implicit and tangled.** `CircuitEvents`
   carries `selectedOperation`, `selectedWire`, `movingControl`,
   `mouseUpOnCircuit`, `disableLeftAutoScroll`, `temporaryDropzones`
   as loose fields. Each pointer interaction reads/writes a different
   subset. There's no explicit state machine, so edge cases (drag
   from toolbox vs. drag existing vs. add control) duplicate logic.
5. **Drag uses raw mousedown / mousemove / mouseup with a manually-
   positioned ghost div.** No PointerEvents capture, so dragging
   behaves oddly when the cursor leaves the SVG, when scrolling, or
   on touch devices.
6. **Multi-target authoring is unreachable from drag-and-drop.**
   Toolbox drop always creates a 1-target gate
   ([`toolboxMousedownHandler`](editor/events.ts)). Per-op temporary
   dropzones exist when _moving_ an existing multi-target gate
   ([`_addGateElementsEvents`](editor/events.ts)) but cannot add a new
   target.
7. **Zero unit-test coverage for this surface.** No `*.test.ts` under
   `circuit-vis/`. Any redesign needs to ship with tests since
   regressions would otherwise be invisible.

#### Phased plan

Order matters. Each phase is self-contained, testable in isolation,
and reversible if it goes wrong.

##### Phase A — Lift the "no editing inside groups" restriction — ✅ COMPLETE (via R1+R2)

**Status:** Done. Original Phase A code shipped passing structural
tests but didn't work in the running editor — dropzones inside
expanded groups landed at wrong screen positions. Root cause was a
broken DOM-attribute reverse-engineering layer; rewriting that
layer required architectural cleanup, which is now done.

The original Phase A goal — making nested dropzones actually hit —
is delivered by R1 (LayoutMap) + R2 (consume LayoutMap directly,
delete the broken reverse-engineering layer). See those phases for
the full delivery notes.

**Lesson learned (still applies):** the original Phase A tests
asserted _structure_ (which `data-dropzone-location` attributes
exist) but not _geometry_ (where the dropzone rectangles are
positioned). That's why they passed while the feature was broken
in production. Future tests on this surface must assert pixel
coordinates — see the working principles under "Architecture
refactor".

##### Phase B — Make multi-target dropping reachable

Two options; pick one when we get there.

- **B1 (minimal, preferred):** Drop from toolbox always creates a
  1-target gate as today, but if the gate's `params` / arity say it
  needs more, automatically open the Inspector (Planned item #2) so
  the user can add the remaining targets via pick-mode. Composes
  with the Inspector roadmap rather than competing with it.
- **B2 (drag-based alternative):** Shift+drag from toolbox enters a
  "multi-drop" mode — first drop sets target 0, subsequent clicks on
  wires add targets, Enter commits. Doesn't require the Inspector
  but has discoverability concerns.

Recommendation: **B1**, taken after Planned item #2 (Gate Inspector)
lands.

##### Phase C — State-machine cleanup + PointerEvents — **SUPERSEDED by R5 + R6**

The original Phase C list (PointerEvents, explicit DragController,
hover-based dropzone highlighting, SVG ghost overlay) has been
folded into the architecture refactor:

- PointerEvents + explicit `DragController` → R5
- Hover-based dropzone highlighting → R5 / R6
- SVG ghost overlay (replacing the positioned `<div>`) → R6

Kept here as a reference; do not pursue these items independently.

#### Working principles for this overhaul

These are the rules the user explicitly asked for; they apply to
every phase.

- **Tests-first.** Each phase ships with regression coverage. If a
  change can't be tested, it's a sign the seam is wrong.
- **Phases are independent.** Phase A must not depend on Phase B or
  C, and so on. If a phase's design seems to require touching
  another, stop and re-design.
- **Preserve current behavior on every flow we don't intend to
  change.** Snapshot tests of the produced dropzone locations are
  the cheapest way to catch unintended regressions.
- **No drive-by refactors.** The state-management and PointerEvents
  cleanup is Phase C and _only_ Phase C. It's tempting to clean up
  while passing through; resist.

---

## Planned (in priority order)

### 1. Persistent view state across re-renders — ✅ in-memory done; host persistence deferred

**Status: in-memory layer shipped.** A new
[`ViewState`](data/viewState.ts) type sits as a third state layer
alongside `CircuitModel` (Data) and `InteractionState` (Action).
[sqore.ts](sqore.ts) holds `viewState: ViewState`, the chevron
click handler writes to it, and `renderCircuit` applies it on top
of the default-expansion passes. `expandOperation` /
`collapseOperation` private methods are gone; the `circuit?`
overload of `renderCircuit` is gone (it existed only to keep
chevron mutations alive across one render — that workaround is
now unnecessary). Locked down by 11 unit tests in
[test/circuit-editor/viewState.test.mjs](../../test/circuit-editor/viewState.test.mjs)
plus an integration test in
[test/circuit-editor/dropzones.test.mjs](../../test/circuit-editor/dropzones.test.mjs)
that fires a real chevron click and verifies the expand survives a
subsequent editor-mutation re-render.

**Status: external circuit updates handled.** The original
in-memory layer fell over for VS Code undo/redo: the React wrapper
in [circuit.tsx](../circuit.tsx) was tearing down the SVG and
constructing a fresh `Sqore` for every external `circuitGroup`
change, which destroyed `viewState` and caused a "Rendering..."
flicker. Fix: a new `Sqore.updateCircuit(group)` swaps the
underlying circuit and re-renders in place, preserving
`viewState`. `ZoomableCircuit` now calls `updateCircuit` on
subsequent prop changes instead of wiping `innerHTML`. Locked
down by an integration test in
[test/circuit-editor/dropzones.test.mjs](../../test/circuit-editor/dropzones.test.mjs)
that fires a chevron click, simulates a host-pushed circuit
update, and verifies the expand survives.

**Known limitation:** entries are keyed by location string. When an
edit shifts an op's position, its `ViewState` entry stays at the
old key and silently goes stale. Stable IDs (R4's `Location` value
type set up the centralization needed for this) are the long-term
fix.

**Other state types to migrate as they land** (no work needed
until each feature):

- Inspector panel: which gate is pinned, which tab is active (#2).
- Multi-select set (#3).
- Zoom level / scroll position (currently re-derived on resize).
- Custom-gate palette: collapsed/expanded sections (#4).
- Diff/snapshot view toggle, breakpoint markers (long-term).

#### Deferred: host persistence (webview reload / VS Code restart)

`ViewState` lives on the long-running `Sqore` instance, which
itself survives every external circuit update via `updateCircuit`.
What it does **not** survive is a webview reload — closing the
circuit tab and reopening it, reloading the VS Code window, or
restarting VS Code all reset the state to defaults.

Deferred for now. Two reasons:

1. **The visible pain points are gone.** The original undo/redo
   regression is fixed; close-and-reopen is occasional rather than
   constant.
2. **Multi-host considerations.** This circuit editor will soon
   need to be hosted outside VS Code as well, and keeping the
   npm-package surface area small matters. Lifting ownership of
   `ViewState` out of `Sqore` would tax every host with a
   ref-management problem most don't care about.

**If we ever want it**, the right shape is an opt-in callback on
`DrawOptions`, not lifting ownership:

```ts
// DrawOptions
viewState?: {
  initial?: ViewStateSnapshot;                  // restore on mount
  onChange?: (snap: ViewStateSnapshot) => void; // notify host on change
};
```

The host opts in, persists how it wants (`vscode.getState()` /
`setState()`, `localStorage`, nothing). `Sqore` still owns the
live `ViewState`. Hosts that don't care pass nothing and get
today's behavior.

### 2. Gate Inspector panel — multi-target editing

**Goal:** Replace today's ad-hoc context menu + single-input prompt
chain with a unified Inspector panel that can edit every property of
the selected gate, including its target list. This unblocks
multi-target authoring, which the data model already supports
(`Unitary.targets: Register[]`) but the toolbox-drop path doesn't
expose.

**Surfaces touched:**

- [contextMenu.ts](editor/contextMenu.ts) — replace the ad-hoc menu with an
  "Open Inspector" action; keep delete/quick-toggle as fast-paths.
- [events.ts](editor/events.ts) — extract `_startAddingControl` /
  `_startRemovingControl` into a generic `_pickWire(predicate, cb)` so
  adding a target reuses the same flow.
- New `inspector.ts` (or `gateInspector.ts`) — owns the panel DOM and
  the in-flight edit state; commits via the existing
  `circuitEvents.renderFn()`.
- [draggable.ts](editor/draggable.ts) — drop continues to create a 1-target
  gate; multi-target is opt-in via the Inspector for now.

**Validation rules baked into the Inspector:**

- No qubit ID may appear in both `targets` and `controls` of the same
  op.
- For built-in gates with fixed arity (`H`, `X`, `M`, …), lock the
  target count.
- For custom gates (see #3), arity comes from the gate's recorded
  signature.

**"Iteration" surface (deferred for now):** the user mentioned exposing
the trace's iteration markers as a first-class authoring concept. Treat
that as its own structural-group authoring item below — the Inspector
should not be blocked on it.

### 3. Snapshot tool — extract selection into a custom gate

**Goal:** User selects a region of the canvas, hits "Create custom
gate from selection", and the selection collapses into a single
multi-target gate node whose body lives in `children`.

**Steps:**

1. **Selection model** on `CircuitEvents` (marquee or shift-click).
   For v1, require a contiguous rectangle: contiguous columns × set
   of wires.
2. **Extraction transform** — pure function on `ComponentGrid`:
   - `targets` = sorted union of every wire touched by the selection.
   - `controls` = empty (user re-adds via the Inspector).
   - Build a new `Unitary` with user-supplied `gate` name, those
     targets, and `children = <selected sub-grid>`.
   - Renumber qubit IDs inside the children to be relative — same
     algorithm as Rust's
     [`synthesize_circuit_for_extraction`](../../../compiler/qsc_circuit/src/circuit_to_qsharp.rs).
   - Validate: every measurement result that exits the selection
     becomes a return value of the new gate.
3. **Persistence**, user picks at extraction time:
   - **In-document only** — body lives in `children`, self-contained.
     The emitter already handles this end-to-end.
   - **Save as separate `.qsc`** — write the body to its own file via
     the existing "Save as Circuit" plumbing
     ([circuit.ts](../../../../vscode/src/circuit.ts)) and reference
     it by name only in the parent.

**Critical test:** round-trip — extracting a sub-region then
re-inlining its `children` back where it stood must produce a
structurally identical grid.

### 4. Custom-gate palette in the toolbox

**Goal:** A second toolbox section listing the document's custom
gates (in-document defs + sibling `.qsc` files). Drag-from-palette
creates a 1-target placeholder; user uses the Inspector to add the
remaining targets to match the gate's arity.

Depends on #3 producing well-formed defs.

### 5. Structural-group authoring (`for` / `if`)

**Goal:** The editor learns to author `loop:` and `if:` groups
natively, replacing the `// loop: …` and `// if: …` comment fallbacks
in the Q# preview with real `for` / `if` blocks. Also covers the
"iteration" concept the user wants exposed in the popup editor.

**Surfaces:**

- Toolbox: new "structural" tile category (loop, conditional).
- Drop creates an empty group node the user fills via drag-into.
- [process_components](../../../compiler/qsc_circuit/src/circuit_to_qsharp.rs)
  in the Rust emitter already inlines these as comments — graduate
  the `loop:` case to emit a real `for` and the `if:` case to emit a
  real `if`/`else`. Existing divergence-banner machinery already flags
  shapes that can't round-trip cleanly.

### 6. Controlled-Adjoint extracted-gate test coverage

**Goal:** Add unit coverage for the
`Controlled Adjoint Foo([c], [qs[0], qs[1]])` shape specifically.
Plain controlled and plain adjoint are covered; the combination is
not.

**Surface:**
[circuit_to_qsharp/tests.rs](../../../compiler/qsc_circuit/src/circuit_to_qsharp/tests.rs).

### 7. VS Code integration tests for the preview pipeline

**Goal:** Today's coverage is heavy on the Rust side and almost
nothing on the VS Code side. Add tests under
[vscode/test](../../../../vscode/test) that exercise:

- `circuitPreviewUriFor` round-trip through `_sourceLookup`.
- Lazy regeneration on first load (open `.qsc`, then open preview
  before the editor has cached anything).
- Filename sanitization end-to-end (open
  `GroupSplittingTest.Main.qsc`, confirm the preview uses the
  sanitized identifier).

### 8. Round-trip validation: `.qs` → `.qsc` → preview Q# matches `.qs`

**Goal:** Currently each direction is tested independently. Add a
test (likely in the Rust crate, fed by snapshot data) that takes a
canonical `.qs`, traces it to a circuit, saves as `.qsc`, regenerates
Q#, and confirms structural equivalence with the original. Catches
emitter regressions that don't surface as compile errors.

### 9. CHANGELOG / release notes

**Goal:** Surface the editor-parity work to users. Should mention
custom-gate extraction, the live preview, the Save-as-Circuit bridge,
and the divergence banner.

---

## Open questions

- Should the Inspector's structural-group authoring (#4) be one
  surface or split into "loop authoring" / "conditional authoring"
  separately?
- Custom-gate palette (#3): scan the workspace for `.qsc` files
  eagerly, or lazily on toolbox-open? Workspace scan adds latency to
  editor startup; lazy adds latency to first toolbox use.
- Multi-document custom-gate references — when a `.qsc` is saved as a
  separate file (#3 option B), where does the parent record the
  reference? Filename only, or content hash too?
