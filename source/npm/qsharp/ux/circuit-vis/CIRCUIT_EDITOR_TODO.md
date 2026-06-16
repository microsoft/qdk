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

#### Known issues — groups & multi-target gates

Discovered during the post-architecture-refactor bug bash on
[GroupSplittingTest.Main.qsc](file:///c%3A/Repos/CustomQsharpFiles/GroupSplittingTest.Main.qsc).
The headline crashes (auto-collapse on move, phantom duplicate,
rapid-drag bad state, group-contents-stuck, classical-ref-in-
targets re-pointing, negative-wire drag) are resolved. The
remaining items are a mix of clear bugs and design questions
that need a UX decision before implementation.

Listed in the order we plan to attack them. Each item has an
"open question" line where the answer isn't obvious; settle the
question before writing code.

##### D1. Crash when a group is emptied by a move-out

**Symptom.** Drag the last remaining child out of a group; the
group becomes empty and the next render throws.

**Likely cause.** After
[`moveOperation`](actions/circuitActions.ts) removes the original
child, the parent group is left with `children: [{ components: [] }]`
(or an entirely empty `children` array). The renderer or one of
the post-move sweeps (`getChildTargets`, the parent-targets
refresh, the measurement-line sweep, `removeTrailingUnusedQubits`)
trips on the empty-children invariant.

**Fix direction.** After the move-out settles, walk up the parent
chain from the source location and delete any ancestor whose
`children` collapsed to empty / all-empty columns. Deletion must
itself cascade — removing one ancestor may empty its parent.

**Open question.** None — empty groups have no rendering and no
semantic meaning; deletion is correct. Confirm with the user
that they're OK with the group quietly disappearing once empty
(alternative: leave a placeholder, which is uglier).

##### D2. Move group containing a classical condition above its producer ✅ SHIPPED

**Symptom.** Drag a group containing `if (c_0)` to a column at or
before the `M` that produces `c_0`. The conditional now
references a classical register that doesn't exist yet at the
consumer's column — the renderer either crashes
("Classical register ID N invalid for qubit ID M with 0
classical register(s)") or produces a semantically broken
circuit.

**Fix.** Two layers, both enforcing the same rule:

> Every external classical-register producer of the moved subtree
> must live in a column strictly earlier than the candidate drop
> target — at every shared level of nesting.

1. **Dropzone-filter (UX).**
   [`DragController.hideInvalidDropzones`](editor/controllers/dragController.ts)
   runs at the end of `onGateMouseDown`. It collects external
   producer locations of the selected subtree via
   [`collectExternalProducerLocations`](actions/circuitActions.ts)
   and sets `display: none` on every `.dropzone` whose
   `data-dropzone-location` would violate the rule. Invalid
   drop targets don't paint and don't catch mouseup, so the user
   never gets a chance to commit an invalid move.

2. **`moveOperation` refusal (safety net).** Same check, applied
   on the action layer. Returns `null` (no-op) if any external
   producer fails the comparison. Catches anything that bypasses
   the UI filter (programmatic moves, future call sites, the
   per-op temporary dropzones the multi-target drag spawns).

**Comparison primitive: column-strict, ancestor-aware.**
[`Location.inEarlierColumnThan`](data/location.ts) walks segments
from the root. The first pair of differing column indices
decides: producer's column < target's column → allowed; equal or
greater → refused. Critically:

- Different ops in the **same** column are simultaneous, not
  predecessor/successor. The user cannot "promote" a consumer up
  to a sibling op-position of the producer's outer group at the
  same top-level column.
- Ancestor groups project their column down onto everything they
  contain. A producer at `"0,0-1,0-0,0-1,0"` (deep inside a
  `for` at top-level col 0) still has top-level col 0 as its
  effective column.

The generic doc-order comparator
[`Location.before`](data/location.ts) was deliberately _not_
reused for this purpose; it would allow same-column siblings and
promote-around-the-rule attacks. Doc-comment on `.before`
explicitly points readers at `inEarlierColumnThan` for this
use case.

**Producer collection.**
[`collectExternalProducerLocations`](actions/circuitActions.ts)
combines two helpers:

- `_collectInternalClassicalRegs` walks the subtree to find
  every `(qubit, result)` reference it consumes.
- `_indexProducers` walks the full grid to build a
  `Map<"qubit:result", locationString>` of where each register
  is produced.

Set-subtracting internal producers from the consumed set leaves
only the external ones — the ones that impose a drop-target
constraint. Internal producers travel with the subtree when it
moves, so they don't constrain anything.

**Visibility-reset hygiene.** The dropzone-filter pass sets
inline `display: none` on individual dropzones. Without a reset,
a drag that doesn't trigger a re-render — canceled drag, or a
drop where `deepEqual` short-circuits `renderFn` — would leave
those marks behind, and the next drag (especially a _toolbox_
drag, which doesn't run the filter) would inherit them and
mysteriously refuse valid drops. Reset happens in
[`installLayerListeners`](editor/controllers/dragController.ts)'s
container-mouseup teardown, alongside the layer-display reset.

**Tests.** Nine regression tests in
[circuitActions.test.mjs](../../test/circuit-editor/circuitActions.test.mjs)
cover: `Location.before` (kept for the generic comparator),
`Location.inEarlierColumnThan` (column-strict, ancestor-aware),
external producer collection (with internal exclusion), refusal,
allow-after, allow-internal, and the promote-around-the-rule
boundary.

##### D5. Dropzone overlapping rendered gate ✅ SHIPPED

Surfaced during D2 testing, but a distinct dropzone-generation
bug independent of D2's classical-ordering rule.

**Symptom.** A full-width central dropzone box renders directly
on top of the first gate of a `for`-iteration's expanded group
(and any other column whose ops aren't sorted top-to-bottom by
wire). Affects the visual and the hit-test — clicking the gate
goes to the dropzone instead.

**Root cause.** The previous
[`_populateDropzonesForGrid`](editor/draggable.ts) algorithm
walked ops in declared order and accumulated a single
monotonically-increasing `wireIndex` across them. The "above me"
suppression (`if (wireIndex < op.minTarget)`) only checked the
_current_ op — it didn't know about other ops in the same
column. When the compiler emits a column like
`[X@wire2, H@wire0]` (execution order, not wire order),
processing `X` (minTarget=2) emits central dropzones at wires 0
and 1, and one of those wires is occupied by the `H` later in
the same array. The dropzone visually lands on the H.

**Fix.** Replace the op-by-op accumulator with a wire-by-wire
pass that uses a precomputed per-column occupancy set:

1. Walk the column's ops once to build
   `occupiedWires: Set<number>` and
   `wireOwnerOpIndex: Map<number, opIndex>`.
2. Iterate wires `[minWire, maxWire)` directly. Always emit the
   inter-column band (it's a narrow strip, overlap is intentional);
   emit the central full-width box only at wires not in
   `occupiedWires`.
3. Recursion into expanded children moved into a separate
   forEach — recursion depends only on op identity and wire
   extent, not on `wireIndex`.

Side benefit: ops in unsorted columns now get dropzones at all
their wires (previously some were silently missed because the
shared `wireIndex` had advanced past them).

##### D3. Multi-target gate / group movement semantics

**Status: ✅ Shipped (pending user-confirmation).**

**Shipped solution.** Kept unit-shift as the design contract and
locked it in with documentation + a bug fix that had been silently
degrading it into the rejected "pin top wire" alternative.

1. **Doc-update on [`_moveY`](actions/circuitActions.ts).** Added
   a `///`-level docblock spelling out:
   - The grabbed wire is the **handle**; the whole op slides by
     `targetWire - sourceWire`.
   - Single-leg movement (the `movingControl` branch) is the
     escape hatch for detaching one register without dragging the
     whole gate.
   - The alternatives we explicitly rejected
     (_pin lowest wire to drop wire_, _resize one leg_) and why.
2. **Closest-wire-to-click in
   [`SelectionController.pickSelectedWire`](editor/controllers/selectionController.ts).**
   The static `data-wire` attribute set by
   [`_addDataWires`](editor/draggable.ts) is the **topmost** wire
   of any multi-wire span — an artifact of its
   `findIndex`-on-`includes` shortcut. Reading it directly on
   group / SWAP / multi-qubit-measurement bodies silently turned
   D3's unit-shift ("grabbed wire is the handle") into
   "pin top wire to drop wire" — the alternative the doc-update
   above had just rejected.
   - Fixed by projecting the click's `(clientX, clientY)` into
     SVG coords via `getScreenCTM().inverse()` +
     `DOMPoint.matrixTransform`, then picking the wire whose Y is
     closest to the click via the new
     [`pickClosestWireIndex`](utils.ts) helper.
   - Single-wire host elems (control dots, target circles, ket
     boxes) skip the projection and read `data-wire` directly —
     no behavior change for them.
   - Falls back to the static `data-wire` if `getScreenCTM()`
     returns `null` (detached SVG) or the closest-wire lookup
     can't reconcile with `wireData` (table mismatch). The click
     still resolves _some_ wire; it just won't be the closest one.
3. **New helpers in [`utils.ts`](utils.ts).**
   - `parseWireYs(elem)` — JSON-parses `data-wire-ys` with the
     same "fail-soft to `[]`" contract `_wireYs` in
     [`draggable.ts`](editor/draggable.ts) already uses, so the
     controller doesn't duplicate the parse.
   - `pickClosestWireIndex(clickSvgY, wireYs, wireData)` — pure
     numerics. Tie-breaks equidistant clicks by smaller `wireY`
     (deterministic) and clamps clicks outside the span to the
     nearest endpoint naturally (no special-case code).
4. **Test coverage.** 20 new tests:
   - 12 in [`utils.test.mjs`](../../test/circuit-editor/utils.test.mjs)
     covering `pickClosestWireIndex` (empty / single / multi /
     tie-break / clamping / ordering-invariance / wireData
     mismatch / duplicate-Y) and `parseWireYs` (missing attr /
     valid / malformed JSON / non-number entries / non-array).
   - 8 in
     [`selectionController.test.mjs`](../../test/circuit-editor/selectionController.test.mjs)
     covering the multi-wire path: top / middle / bottom picks,
     above / below clamping, CTM-null fallback, wireData mismatch
     fallback, single-wire skip. Tests stub `DOMPoint` + the CTM
     by hand since JSDOM ships neither.

**Why not the alternatives.** Recorded here so the next reader
doesn't waste a cycle re-deriving them:

- _Pin lowest wire to drop wire._ Predictable for "I want this
  group at wires 2..5" mental model, but it's exactly what the
  `data-wire`-topmost shortcut was accidentally giving us — and
  it felt wrong in practice. Removed by D3 step 2 above.
- _Resize (one leg moves, others stay)._ Only meaningful for
  multi-target gates with a clear "main" wire. Probably belongs
  in the Inspector (Planned item #2), not the drag-and-drop
  surface.

**Out of scope.** Multi-target authoring beyond shifting
(resize, add/remove leg) still belongs in the Inspector. D3
just makes the shift-semantics path match its design intent.

##### D4. Move-inside-group vs. promote-out-of-group disambiguation

**Status: Stage A shipped (user-confirmed). Stage B planned.**

A design pass reframed this. The original framing
("which of options a–d?") was too narrow; the actual gap is
that **there's no clean drag gesture for "extend the group to
cover a new wire / column."** Every other group-related drag
gesture has a coherent default — but "extend" sits on top of
two other drag intents ("promote out" and "just place near the
group") and has nothing to distinguish itself.

###### User-intent matrix

Strip out implementation details and what's reachable today, and
list only what users might **want** to do. Restricted to a single
group with no nesting (nested groups inherit the same rules one
scope at a time).

**A. Source gate is outside the group.**

| #   | Drop location relative to the group              | Intent                                                                                           |
| --- | ------------------------------------------------ | ------------------------------------------------------------------------------------------------ |
| A1  | Another external position                        | Plain move. Group not involved.                                                                  |
| A2  | On a wire AND column the group already spans     | "Add this gate to the group."                                                                    |
| A3  | On a wire the group spans, column adjacent       | "Add this gate to the group, extending it sideways to swallow the new column."                   |
| A4  | On a wire the group does NOT span, column inside | "Extend the group vertically and absorb me" — _or_ — "keep me outside, I'm just placing nearby." |
| A5  | Corner-adjacent (off-wire AND off-column)        | Almost always "keep outside, near the group."                                                    |

**B. Source gate is inside the group.**

| #   | Drop location relative to the group                | Intent                                                                             |
| --- | -------------------------------------------------- | ---------------------------------------------------------------------------------- |
| B1  | Elsewhere inside the group's rectangle             | Rearrange within group. Group may shrink if the move freed up a wire/column.       |
| B2  | On a covered wire, column outside the group        | "Promote out" — _or_ — "extend group sideways, keep me inside."                    |
| B3  | On a wire NOT covered, column inside the group     | "Promote out, side-step group" — _or_ — "extend group vertically, keep me inside." |
| B4  | Far away (different wire AND different column)     | Almost always "promote out entirely."                                              |
| B5  | A move that would leave the group with no children | "I'm done with this group — let it dissolve." (D1 cleanup handles the prune.)      |

**C. Source is the group itself.** Whole-group drag (D3 unit-shift).
Not in D4's scope.

**D. Membership change without moving any gate.** Belongs in the
gate-edit panel (Planned item #2) or selection-based "wrap in
group" tooling, not D4. Specifically out of scope here:

| #   | Operation                              | Owned by                       |
| --- | -------------------------------------- | ------------------------------ |
| D1  | "Add this external gate to the group"  | Gate-edit panel                |
| D2  | "Remove this internal gate from group" | Gate-edit panel                |
| D3  | "Dissolve the group, keep contents"    | Gate-edit panel / context menu |
| D4  | "Wrap selected gates in a new group"   | Snapshot / selection tooling   |

**E. Resize the group's box directly.** Top/bottom/left/right
edge drag handles. Different gesture entirely from D4 (no gate
is being moved). Possible follow-up; not in scope here.

###### The ambiguous gestures

Three rows from the matrix above have two equally-plausible
intents from one gesture: **A4**, **B2**, **B3**. Of these, B2
collapses out once we add a leading/trailing inner-column
dropzone band (see Stage A below) — drop in the band = inside,
drop further out = outside. That leaves **A4** and **B3** as
the genuinely ambiguous "extend group vertically" cases, which
need a modifier (see Stage B). A4 is sufficiently rare and the
intent sufficiently weak that we'll **not** support it in this
pass; vertical extend is internal-source only.

###### Design decisions

- **Default rule stays geometry-based.** Inner-scope dropzones
  inside the group's rendered rectangle ⇒ stay in the group;
  outer-scope dropzones outside it ⇒ promote out. This is the
  rule already in place via `_dropzoneLayer`'s scope-clamping;
  D4 just rounds out the gaps in it.
- **Inner-column dropzones on BOTH sides of every expanded group.**
  Today the left-side leading-column band already works (it's
  the natural left edge of the group's first column). The
  right-side trailing band is the missing mirror. Reach: one
  column past either edge, unconditional (no shift needed),
  visually undifferentiated from other inner-scope dropzones.
  Covers A3 and B2-as-extend with no modifier.
- **Shift modifier = "extend the group vertically to cover the
  drop wire."** Internal-source only (B3-as-extend). Read at
  mouseup via `ev.shiftKey`, but tracked live during drag via
  keydown/keyup for the visual feedback (see below). No reach
  cap on the drop wire: shift-dragging to a wire several rows
  outside the group's current span legitimately extends the
  group to cover all the intervening wires. Any gate already
  occupying one of those wires that "shouldn't" be in the group
  gets bumped — same way control-line crossings already shift
  unrelated gates today.
- **Multi-wire sources are not a special case.** A two-target
  op (CNOT, multi-qubit measurement, sub-group) shift-dragged
  by one of its legs lands all its legs at the post-D3-unit-shift
  positions, and the group extends to cover the **full** new
  wire span of the dragged op, not just the grabbed leg. Same
  code path as single-wire sources.
- **Ghost-border visual feedback for shift.** While shift is
  held mid-drag and the cursor is over an inner-scope dropzone,
  draw a translucent extension of the group's border out to the
  hovered wire — the user sees exactly which wires would be
  swallowed if they released now. Released without shift, the
  ghost vanishes and the regular promote-out path fires.
  Releasing on a dropzone the shift-extend rule doesn't cover
  (corner-adjacent, far away, etc.) also falls through to
  regular logic — shift is silently ignored, no error state.
- **D4-D items (membership change without moving) explicitly
  deferred.** No D4 work attempts a non-drag affordance.

###### Phased implementation

Two PRs, sequenced.

- **Stage A: right-side trailing inner-column for groups. ✅ Shipped
  (user-confirmed).** Unified the previously-top-level-only
  `_appendTrailingColumn` into a per-scope helper
  `_appendTrailingColumnForScope` in
  [`draggable.ts`](editor/draggable.ts), called from inside
  `_populateDropzonesForGrid` once per scope. Every expanded group
  now gets a trailing inner-column band (one column past its
  rightmost child column, clamped to the group's wire span) at the
  same recursion depth as the inner-scope dropzones the loop
  already emits. Top-level trailing band behavior is unchanged — it
  now flows through the unified path instead of its own one-shot
  call from `_dropzoneLayer`, but the emitted dropzones are
  byte-for-byte identical (locked down by a regression test).
  - No action-layer changes were needed; `_addOp`'s existing
    "create column if absent" branch already accepts inner-scope
    location strings whose colIndex is one past the rightmost.
  - Same styling as existing inner-scope dropzones — geometry
    reads ("snug against the right edge of the group's box")
    without dedicated CSS work.
  - Test coverage: 4 new tests in
    [test/circuit-editor/dropzones.test.mjs](../../test/circuit-editor/dropzones.test.mjs)
    (emission, wire-extent clipping, collapsed-group no-emission,
    top-level trailing-band preservation) and 3 new tests in
    [test/circuit-editor/circuitActions.test.mjs](../../test/circuit-editor/circuitActions.test.mjs)
    (`addOperation` to a group's trailing inner slot, external
    gate move into a group via the slot, internal gate move
    within a group via the slot). 306 tests passing (up from 299).
- **Stage B: shift-to-extend-vertically for internal sources. ✅
  Shipped (pending user-confirmation).** Built atop Stage A's
  per-scope dropzone scaffolding. The full plan as designed below
  was implemented faithfully; only minor behavioral details surfaced
  during testing (documented inline in the test file) — see the
  `circuitActions extend:` tests for ground-truth semantics.

  Layer-by-layer landing notes:
  - **Action layer**
    ([`circuitActions.ts`](actions/circuitActions.ts)): After the
    move, source-side ancestor refresh, and empty-prune settle, a
    new `_extendDestAncestorsVertically` helper walks the
    destination's pre-captured ancestor chain innermost-out,
    refreshing each ancestor's derived targets via the existing
    `_refreshDerivedTargets` and stopping at the first ancestor
    whose pre-existing span already encloses its (now-widened)
    child. Pruned ancestors are silently skipped (B5 case). A
    companion helper `_collectDestAncestorChain` captures the
    chain _before_ mutation by walking parsed `Location` prefixes.
    A second companion helper `_resolveOverlapAfterExtend` runs
    after each refresh — if widening the ancestor's `.targets`
    now overlaps a sibling op in the same column, it splices the
    ancestor into a fresh column inserted at the same column
    index, leaving the surviving siblings one slot to the right.
    Mirrors `commitAddControl`'s split-and-shift convention so the
    two "operation-grew-its-span" code paths feel the same.

    The cascade runs unconditionally on every move. The target
    location string is authoritative — if the user dropped the
    source inside group G, then G IS the source's new parent and
    G's `.targets` MUST reflect that, regardless of whether the
    drop wire was inside or outside G's pre-move span. An earlier
    iteration gated the cascade on an `extendDestGroupVertically`
    opt-in flag (set by the dragController when the user released
    on a `data-shift-extend` dropzone), but that conflated
    correctness ("keep ancestors' `.targets` in sync with their
    actual children") with UI intent ("offer drop targets on
    off-span wires"). The UI piece still belongs in the
    controller — shift gates the visibility of off-span dropzones
    via the shift-extend scaffolding — but the action layer just
    needs to honor whatever location string it receives.

  - **Geometry helper**
    ([`draggable.ts`](editor/draggable.ts)): new
    `makeShiftExtendGhost(scope, wireData, groupMinWire,
groupMaxWire, hoverWireIndex, hoverColIndex)` exports a single
    translucent `<rect>` covering G's columns (extended one column
    right when hovering the trailing-append slot) and Y span
    extended to enclose the hover wire, padded by
    `DROPZONE_PADDING_Y`. Reads everything from the LayoutScope —
    no DOM querying of G's rendered box.
  - **DragController** ([`dragController.ts`](editor/controllers/dragController.ts)):
    5 new private fields (`_shiftExtendCtx`,
    `_shiftExtendDropzones`, `_ghostBorder`, `_onShiftDown`,
    `_onShiftUp`) and 6 new private methods (`setupShiftExtend`,
    `tearDownShiftExtend`, `spawnShiftExtendDropzones`,
    `clearShiftExtendDropzones`, `paintGhostBorder`,
    `clearGhostBorder`). `setupShiftExtend` wires into
    `onGateMouseDown` after `hideInvalidDropzones`;
    `tearDownShiftExtend` runs from the container mouseup handler
    in `installLayerListeners`. Document keydown/keyup listeners
    spawn or clear shift-extend dropzones; their
    `mouseenter`/`mouseleave` (re)paint and clear the
    ghost-border. `onDropzoneMouseUp` detects
    `isShiftExtend = ev.shiftKey && dropzoneElem.getAttribute("data-shift-extend") === "true"`
    and passes the boolean as the new 9th `moveOperation`
    argument on the non-copying move path.
  - **CSS** ([`qsharp-circuit.css`](../qsharp-circuit.css)): new
    `.shift-extend-ghost` rule — translucent fill, dashed border,
    `pointer-events: none`.
  - **Test coverage**: 11 new action-layer tests in
    [test/circuit-editor/circuitActions.test.mjs](../../test/circuit-editor/circuitActions.test.mjs)
    covering the basic single-wire extend, multi-row gap extend,
    multi-wire source extend, nested cascade, cascade early-exit,
    empty-group B5 prune, the load-bearing cross-chain case where
    source lives outside the destination group (the scenario where
    the flag is actually load-bearing — in same-chain moves, the
    existing source-side ancestor refresh already extends G), and
    4 collision-split tests: single sibling collision, no-collision
    no-op, multiple-sibling collision (all siblings stay together
    in the right column), and nested-ancestor collision split (a
    deep ancestor splits its OWN containing column on cascade).
    All 317 npm tests pass (306 before → 317).
  - **Behavioral subtlety surfaced during testing.** The cascade
    refreshes each ancestor's `.targets` from `getChildTargets`,
    which returns _exactly_ the wires its descendants reference —
    no phantom wires. So a single-child shift-extend that lands
    the child on a previously-uncovered wire may also _shrink_ G
    along axes where no descendant remains. This is the
    children-derived contract behaving correctly. For multi-child
    groups, the extend cleanly grows the span without losing any
    existing wires.

  The original design as planned follows below for reference.

  **Detection.** Source is "internal" iff its location string has
  at least one `-` separator (nested at least one level deep). The
  "host group" G whose span will extend is the immediate parent —
  the op at the location prefix before the last `-`. Nested deeper?
  Only the immediate parent extends in response to user intent; any
  _ancestor_ G' that no longer visually encloses G after the extend
  also extends, as a cascade (see "Cascade up" below) so the picture
  stays consistent — but that cascade is automatic, not user-driven.

  **Shift-extend dropzones (drag-time, not render-time).** When
  shift is held mid-drag with an internal source, the dragController
  spawns temporary dropzones in G's scope at every `(column, wire)`
  where:
  - `column` is one of G's existing inner columns (including the
    trailing-append column Stage A added), and
  - `wire` is in `[0, wireData.length)` but **not** in G's current
    `[minTarget, maxTarget]` span — i.e. precisely the wires Stage A's
    wire-clamp suppresses from inner emission.

  Each shift-extend dropzone gets `data-shift-extend="true"` so the
  mouseup handler can tell them apart from regular dropzones. They
  share Stage A's `data-dropzone-inter-column="false"` (drop, don't
  insert-between) and reuse `makeDropzoneBox`'s on-column geometry —
  no new geometry math, no new styling.

  Spawned via `trackTemporaryDropzone(this.ctx.interaction, ...)` so
  the existing teardown path (`clearTemporaryDropzones`, fired in the
  container mouseup) cleans them up. Re-spawned on every shift-down
  during the same drag; cleared on shift-up.

  **Ghost-border overlay.** A single translucent `<rect>` painted in
  the editor overlay layer when shift is held AND the cursor is over
  a shift-extend dropzone. Computed from `LayoutMap` (same source as
  the dropzones — no DOM querying of G's rendered `<rect>`):
  - X span: G's leftmost column's `colStartX` to its rightmost
    column's `colStartX + colWidth` (extended one column right if the
    hovered dropzone is the trailing-append column).
  - Y span: `min(G's top wire Y, hover wire Y) - DROPZONE_PADDING_Y`
    to `max(G's bottom wire Y, hover wire Y) + DROPZONE_PADDING_Y`.

  Removed on shift release, on hover-off (mouseleave on the
  shift-extend dropzone), and on mouseup (container teardown).

  **Live shift tracking.** Document keydown/keyup listeners installed
  on drag start (in `onGateMouseDown` / `onToolboxMouseDown`),
  removed on container mouseup. They (re)spawn or clear shift-extend
  dropzones and (re)paint or clear the ghost-border. `ev.shiftKey` at
  mouseup remains the source of truth for the action decision.

  **Action layer.** `moveOperation` always re-derives each
  destination ancestor's `.targets` from its post-move children
  via `getChildTargets`. The rebuild cascades upward: each
  ancestor whose `.targets` no longer encloses its (now-widened)
  child gets its `.targets` rebuilt too. Walk terminates at the
  top-level grid or at the first ancestor whose pre-existing span
  already encloses the child below it. No reach cap on the drop
  wire; the cascade keeps the visual enclosure invariant
  regardless of how far the drop is from G's current span.

  The cascade is correctness, not opt-in policy — an ancestor's
  `.targets` must always reflect its actual children, and the
  target location string is the authoritative statement of which
  group the moved op lands in.

  **Empty-group case (B5).** Last-child shift-drag is well-defined:
  the source leaves G, G becomes empty, the existing
  `_pruneEmptyAncestors` sweep removes G entirely. Shift becomes
  moot — the dropzone the user landed on was inside G's old scope,
  which no longer exists, so the action effectively lands the source
  at top level on its new wire (via `_addOp`'s parent-array
  resolution at the time of the move). No special case in the
  controller; the action falls out of the existing empty-prune path.

  **Non-applicable drop (shift ignored).** If shift is held but the
  dropzone the user releases on is not a shift-extend dropzone (it's
  a normal inner / outer / inter-column dropzone), shift is silently
  ignored and the regular move/promote-out path fires. The mouseup
  handler simply doesn't see `data-shift-extend="true"` and skips the
  extend branch.

  **Shift-released-mid-drag.** Keyup clears the shift-extend
  dropzones and the ghost-border. Mouseup polls `ev.shiftKey`
  (false), and the user lands on a regular dropzone — plain drop
  semantics. No state leaks across drags because container mouseup
  unconditionally clears every temporary dropzone and removes the
  ghost-border.

  **Tests planned.**
  - Action layer (dest-side ancestor refresh cascade):
    - Shift+drop internal source to a wire just outside G's span:
      G's `.targets` covers the new wire; source lands in G.
    - Shift+drop to a wire several rows beyond G's span:
      G extends to cover the gap.
    - Shift+drop a multi-wire internal source (e.g. CNOT inside G):
      G extends to cover the moved op's full new wire span.
    - Cascade: shift+drop in nested-group scenario where G's new
      span exceeds G''s — G' also extends, transitively.
    - Empty-group: last-child shift-drop prunes G entirely; the
      cascade is a safe no-op against the pruned chain.
    - Cross-chain: external source dropped inside G on an off-span
      wire — G extends to enclose it (the source-side refresh
      acts on the source's old ancestors, not G, so the dest-side
      cascade is the only thing that keeps G consistent here).
  - Controller / dropzones:
    - Shift-extend dropzones spawn on shift-down during internal
      drag, at all `(column, off-span-wire)` pairs.
    - Cleared on shift-up; cleared on container mouseup.
    - External-source drag (no internal context) doesn't spawn
      any shift-extend dropzones.
  - Integration: a shift+drop+release sequence ends with the
    expected grid state (covered by the action-layer tests; the
    controller wiring is tested via direct dropzone emission rather
    than the full mouseup chain to keep tests in the controller's
    direct-test style).

**Out of scope for this pass.**

- A4 (external gate + shift = extend vertically + absorb). Rare,
  weak intent, easy to fake via two steps. Revisit if asked.
- E (resize the group's box directly via edge handles). Different
  gesture, no gate being moved. Possible follow-up.
- D-items (no-move membership change). Owned by the gate-edit
  panel and selection-based snapshot tooling — separate roadmap
  items.

##### Roadmap & status

| Item                                     | Severity               | Status                      |
| ---------------------------------------- | ---------------------- | --------------------------- |
| D1: empty-group crash                    | Crash                  | ✅ Shipped (user-confirmed) |
| D2: classical condition before producer  | Logic error            | ✅ Shipped (user-confirmed) |
| D3: multi-target semantics               | Design / documentation | ✅ Shipped (pending user)   |
| D4: move-out vs. expand-group            | Design                 | ✅ Shipped (pending user)   |
| D5: dropzone overlapping rendered gate   | Bug                    | ✅ Shipped (user-confirmed) |
| D6: pure-derived group `.targets`        | Refactor               | ❌ Investigated, rejected   |
| D7: centralized ancestor-targets utility | Refactor               | ✅ Shipped (pending user)   |

---

### D6 — Pure-derived group `.targets` (investigated, rejected)

**Context.** D4 Stage B's dest-side ancestor refresh, plus the
source-side parent refresh inside `moveOperation`, plus the
per-rung refresh in `_pruneEmptyAncestors`, made the action layer
responsible for keeping a group's `.targets` in lockstep with its
children. That's an eager-cache design: `.targets` on a group is a
denormalized union of descendant wires, maintained by every
mutator. A cleaner-looking alternative is **pure-derived** —
group ops have no authoritative `.targets`; readers
(`getMinMaxRegIdx`) descend children at read time. The user's
framing: "a group's targets should always be determined by
their children."

**Outcome: rejected.** A full implementation built end-to-end
(action-layer cleanup, subtree-walking `getMinMaxRegIdx`,
save-time recompute in `Sqore.minimizeOperation`, 318/318 tests
green, snapshots byte-identical), benchmarked, and reviewed.
Decision after review: **keep the eager cache**. Reasons:

1. **Performance cost is real.** Benchmark in
   [circuitTargets.bench.md](../../test/circuit-editor/circuitTargets.bench.md):
   render 1.7×–2.5× slower, mutate 1.4×–3.3× slower vs.
   baseline-eager across six scenarios. The renderer/resolver
   hot paths previously O(1) (read cached `.targets`) become
   O(descendant count). Renders run on every keystroke during
   drag/drop; the slowdown isn't invisible.
2. **Semantic clarity got worse, not better.** Pure-derived
   leaves `.targets` populated in the JSON schema, populated in
   the file format, populated on every op in memory — **and
   ignored by the runtime**. The first reader who hits
   `op.targets` and trusts it gets a surprise. The save-time
   recompute inside `Sqore.minimizeOperation` is a fragile
   invariant — easy to forget when adding a new save path.
3. **The motivating bugs were fixable in the eager model.** The
   investigation precisely diagnosed them, and none required
   redesigning the data model: cascade refresh ordering
   (refresh-before-mutate vs. refresh-after-mutate), the
   `getChildTargets` strip-`result` bug that silently dropped
   classical refs, and empty-prune needing to run before parent
   refresh.

**What we keep from the experiment.** The work isn't wasted —
several artifacts ship as standalone improvements:

- **`getChildTargets` `result`-preservation fix.** Reuse the
  same fix on the eager-cache side; it's a one-line correctness
  bug that exists regardless of the data model choice.
- **Snapshot harness.** The new
  [if-else.qs.snapshot.html](../../test/circuits-cases/if-else.qs.snapshot.html)
  and
  [conditionals.qs.snapshot.html](../../test/circuits-cases/conditionals.qs.snapshot.html)
  baselines (plus the harness that produces them) catch
  rendering regressions for classically-controlled groups —
  which were previously uncovered.
- **Benchmark + `bench.md`.** Becomes the artifact that
  justifies the eager-cache choice; the next contributor who
  asks "why not pure-derived?" sees the numbers and doesn't
  re-litigate.
- **The semantic contract written into the
  [`getMinMaxRegIdx`](utils.ts) doc comment.** Port it back,
  reframed as "`.targets` IS authoritative; this matches what
  the Rust builder seeds via `new_group` + `merge_inputs`." The
  comment is independently valuable.
- **New test scenarios** (12 in
  [circuitActions.test.mjs](../../test/circuit-editor/circuitActions.test.mjs)
  - [dropzones.test.mjs](../../test/circuit-editor/dropzones.test.mjs))
    lock down extend cascade and overlap-split behavior. Rewrite
    the assertions back to direct `.targets` checks (which IS the
    contract under eager cache); the scenarios stay.
- **Action-layer hygiene** that didn't depend on the data-model
  change: empty-prune ordering relative to parent refresh, the
  `_isOperationEmpty` extraction, the
  `_pruneEmptyAncestors` structure improvements.

**Hybrid we explicitly chose not to take.** Per-render
memoization on a pure-derived field claws back some render
perf but adds yet another invariant ("cache valid for the
duration of one draw") and doesn't help mutate at all. More
moving parts to reason about for less benefit than keeping the
cache authoritative.

**Next:** revert the pure-derived edits in
[utils.ts](utils.ts),
[actions/circuitActions.ts](actions/circuitActions.ts), and
[sqore.ts](sqore.ts); cherry-pick the keep-list above onto the
eager-cache baseline; then take **D7** (below) to make the
cache maintenance itself less error-prone.

---

### D7 — Centralized bottom-up ancestor `.targets` refresh utility

**Status: shipped.** The three scattered refresh sites are now a
single [`refreshAncestorTargets`](actions/circuitActions.ts)
walk; `getChildTargets` no longer strips the `result` field
during dedup (see the D6 keep-list item, fixed alongside this
work). 325 npm tests pass — the same 318 from before D6 plus 7
new direct-on-`getChildTargets` tests in
[utils.test.mjs](../../test/circuit-editor/utils.test.mjs) that
lock down the strip-`result` regression.

**Context (before).** D6 confirmed that the eager-cache design
wins on balance, but the **mechanism** by which `moveOperation`
kept a group's `.targets` in sync with its children was spread
across three call sites that each walked the ancestor chain
slightly differently:

1. An inline source-parent refresh inside `moveOperation`
   (`sourceParentOperation.targets = getChildTargets(...)`).
2. `_extendDestAncestorsVertically` — the dest-side cascade
   added by D4 Stage B (innermost-out, span-enclosure
   early-exit, paired with `_resolveOverlapAfterExtend`).
3. The per-rung refresh inside `_pruneEmptyAncestors` (a
   `needsRefresh` flag that ran `_refreshDerivedTargets` on
   whichever rung followed a freshly-deleted ancestor).

Plus the underlying `getChildTargets` itself, which had a silent
bug (strips `result` field from `Register`s, losing
classical-control refs on any refresh).

Each call site re-implemented "find this op's parent, recompute
its `.targets`, decide whether to keep walking up." That was the
shape of a bug factory.

**Delivered.**

1. New private
   [`refreshAncestorTargets(chain, options)`](actions/circuitActions.ts)
   utility — bottom-up walk over a pre-captured `AncestorRung[]`
   that calls `_refreshDerivedTargets` on each still-attached
   ancestor and early-exits at the first rung whose recomputed
   `.targets` matches its old value. Pure data mutation: no DOM,
   no column reshape. Sibling-collision resolution is composed
   via a per-rung `onAfterRefresh` hook, which keeps the utility
   focused on `.targets` and leaves the
   `_resolveOverlapAfterExtend` split-and-shift as the dest-side
   caller's concern.

   The refresh itself is
   [`_computeDerivedTargets`](actions/circuitActions.ts) →
   immediate children only (`getOperationRegisters` on each
   direct child, then dedup), not a full subtree walk. Valid
   because each child's `.targets` is itself the eager cache of
   its subtree, so unioning the immediate children's already-
   correct caches reproduces the full subtree union without
   re-walking it. Termination via change-detection: when a rung's
   recomputed value equals its current cache, no parent above it
   can have changed either, so the walk stops.

   **Shared-ancestor caveat.** `onAfterRefresh` fires on every
   visited still-attached rung regardless of whether the refresh
   produced a change. This is required when source and dest
   chains share an ancestor: the first cascade to reach the
   shared rung writes its new `.targets`; the second cascade then
   sees "unchanged" but the span has still widened relative to
   pre-mutation, and the overlap-resolver hook must still get a
   chance to split a collided sibling column. Termination on
   `!changed` happens AFTER the hook fires on that rung.

2. `_pruneEmptyAncestors` refactored to **prune-only**, returning
   the surviving (still-attached) portion of the chain. The
   inline `needsRefresh` flag is gone; the post-prune refresh is
   now the same `refreshAncestorTargets` call the dest side uses.
3. `moveOperation`'s tail is now a clean two-step:
   ```ts
   const survivedSourceChain = _pruneEmptyAncestors(ancestorChain);
   refreshAncestorTargets(survivedSourceChain);
   refreshAncestorTargets(destAncestorChain, {
     onAfterRefresh: ({ op, containingArray }) =>
       _resolveOverlapAfterExtend(op, containingArray),
   });
   ```
   The inline source-parent refresh + `findParentOperation`
   lookup are removed; the source-side post-prune refresh walk
   covers the same case (and more — it cascades upward when an
   ancestor's span narrows, which the inline single-rung refresh
   could not).
4. `getChildTargets` strip-`result` fix (landed as the D6
   keep-list item just before this work) — dedup is now keyed on
   `(qubit, result)` rather than `qubit` alone, so
   classical-control refs survive every refresh.

**Resolved design questions.**

- **Capture-before-mutate vs. parse-after.** Picked
  capture-before-mutate uniformly. Both source and dest chains
  are captured via `_collectAncestorChain` /
  `_collectDestAncestorChain` at the top of `moveOperation`, and
  the captured `(op, containingArray)` object references survive
  any mid-move column splices or prune cascades that would
  invalidate hierarchical location strings. The
  `stillAttached` check inside `refreshAncestorTargets` handles
  the case where a captured rung was pruned away between
  capture and refresh.
- **Coupling to the overlap-resolver.** The utility itself stays
  pure (`.targets` refresh only). Callers compose
  `_resolveOverlapAfterExtend` via the `onAfterRefresh` hook —
  needed on the dest side because widening can introduce a
  sibling-column collision, not needed on the source side
  because narrowing can't.
- **Idempotency contract.** Refresh is deterministic
  (`_computeDerivedTargets` produces the same array twice in a
  row) and the change-detection early-exit fires immediately on
  the second call (every ancestor's recomputed value equals its
  cache). Documented on the utility's doc comment.

**Tests.** All 12 D6-era assertions in
[circuitActions.test.mjs](../../test/circuit-editor/circuitActions.test.mjs)
continue to pass — they exercise exactly the
cascade-and-refresh behavior the utility now owns. The
end-to-end coverage is sufficient; no separate unit-level test
file for the private utility (its observable behavior IS the
end-to-end behavior). 325/325 tests pass after the
immediate-children optimization, including the three
extend-cascade-with-sibling-split tests (105, 107, 108) that
exercise the shared-ancestor hook-firing contract.

**What didn't change.**

- The on-disk shape: `.targets` / `.results` field semantics,
  the `kind` discriminator's switch, the JSON schema — all
  identical. The refactor is purely about WHO writes those
  fields and WHEN, not what they contain.
- Reader-side perf: `getMinMaxRegIdx` still does an O(1) cache
  read. No per-render memoization (out of scope; see D6's
  hybrid-rejected note).
- The Rust builder. The Rust side already produces correct
  `.targets` on disk; this work was purely about how the npm
  package's action layer maintains them after edits.

---

### Controls on groups — feature

**Design summary (locked in).** Groups (any op with `children`)
may carry **classical controls only** — never quantum controls —
and are never adjointable through the editor surface. This is a
permanent design decision from the team, not a deferral pending
future work. Multi-target unitaries (SWAP-shaped ops, multi-qubit
measurements) likewise can't have quantum controls authored or
removed through the editor, because the CNOT-style "one solid
line from top control to bottom target" rendering rule doesn't
extend to a body split across non-adjacent wires.

What the editor enforces:

- `addControl` / `removeControl` refuse on any op where
  `_isMultiTargetOrGroup(op)` returns true.
- The context menu omits "Add Control" / "Remove Control" /
  "Toggle Adjoint" on those ops (and on every group, regardless
  of subtree contents).
- The renderer doesn't carry special-case logic for
  quantum-control connectors on groups. Externally-supplied data
  (e.g. a `.qsc` file authored elsewhere) that has quantum
  controls on a group renders without those controls — the
  dashed box and child gates draw as normal, the control dot and
  connector just don't appear.

**Why this was non-trivial to land.** "Add a control to a group"
sounds like a one-liner — model has `op.controls`, renderer has
`controlsY`, done. In practice the gesture cut across every layer
of the editor and each layer had assumptions that quietly only
worked for the historical "single-op + controls" shape (CNOT-style
1 target + N controls). Trying to land the feature surfaced a
chain of latent issues that look like bugs in isolation but were
really gaps in a feature with no settled design. The team's
final decision to lock the editor's authoring surface to
single-target unitaries cleanly resolves the cross-layer
ambiguity — the affected layers either need no change or only
need a refusal predicate at the action layer.

**Surface area touched (cumulative).**

- Data: `op.controls` `(qubit, result?)` entries on group ops.
- Action: `addControl` / `removeControl` dedup; `_moveAsUnit`
  branch selection; `_moveY` leg-vs-unit semantics; `moveOperation`
  identity loss across deep-clone.
- View / hit: `dragController.onGateMouseDown` early-return for
  expanded groups; `selectionController.movingControl` flag.
- Renderer: `gateFormatter._groupedOperations` (B5-era
  per-control-routing was reverted in the M2 partial-revert);
  `_classicalControls`; per-control id resolution in
  `process.classicalControlIds`.
- View state: `Sqore.rebaseViewState` identity-keyed remap;
  default-expansion rule for classically-controlled groups.

#### Shipped milestones

Each milestone is a self-contained ship with tests. The
detailed root-cause + fix writeups live in the
[`B`-numbered bug-fix entries](#bug-fixes--open) below — kept there
as engineering archaeology rather than re-inlined here.

| #   | Detail                                                                                                                                                 | One-line summary                                                                                                                                                                                                                                                     |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| M1  | [B5](#b5-add--remove-control-on-a-classically-controlled-op-blocked-by-classical-ref-entries---shipped-pending-user-confirmation)                      | Add / remove control on classically-controlled ops: filter four dedup / dropzone sites for pure-quantum entries. ⚠️ **Group / multi-target half permanently reverted by M5**; single-target-unitary half still in effect.                                            |
| M2  | [B9](#b9-quantum-controls-on-a-group-are-never-drawn---shipped-pending-user-confirmation)                                                              | Rendering: mixed quantum+classical controls render on single-target unitaries. ⚠️ **Group rendering half permanently reverted by the design lock-in** — quantum controls on groups are no longer drawn; the single-target-unitary half (a B5 follow-up) still ships. |
| M3  | [B10](#b10-control-drag-on-a-group-moves-the-whole-group-instead-of-just-the-control---shipped-pending-user-confirmation)                              | Drag semantics: control-leg drag rewires just the leg (not the whole group); drop on body wire swaps via `_swapWiresInSubtree`.                                                                                                                                      |
| M4  | [B11](#b11-control-drag-on-a-group-expanded-groups-blocked--classically-controlled-groups-re-expand-on-every-move---shipped-pending-user-confirmation) | Drag init on expanded groups + ViewState transfer across `moveOperation`'s deep-clone via `sqore-prev-location` stamp.                                                                                                                                               |
| M5  | [see below](#m5-refuse-addremove-control-on-multi-target-ops--groups--shipped)                                                                         | Refuse `addControl` / `removeControl` on any op with `children != null` or `targets.length > 1` (incl. multi-qubit measurements). Action layer + context menu both gated. Permanent design.                                                                          |
| M7  | [see below](#m7-hide-toggle-adjoint-on-groups--shipped)                                                                                                | Hide "Toggle Adjoint" on every group (`children != null`). Permanent design — groups are not adjointable through the editor surface. Leaf unitaries are unaffected.                                                                                                  |

(B8 — clone-move of a multi-wire group rewriting `.targets` —
shipped in parallel but isn't gated on having controls; it's a
general group-cloning fix and stays in the bug-fixes section.)

##### M5: refuse add/remove control on multi-target ops / groups — shipped

**What changed.** Added `_isMultiTargetOrGroup(op)` predicate in
[circuitActions.ts](actions/circuitActions.ts) — returns `true`
when `op.children != null` OR `op.targets.length > 1` (unitary /
ket) OR `op.qubits.length > 1` (measurement). Both
[`addControl`](actions/circuitActions.ts) and
[`removeControl`](actions/circuitActions.ts) early-return `false`
when the predicate fires. The context-menu builder in
[contextMenu.ts](editor/contextMenu.ts) consumes the same
predicate to omit "Add Control" / "Remove Control" /
"Remove control" entries so the menu never advertises a
silently-no-op affordance.

**Why this is permanent.** Trying to extend the editor to author
quantum controls on multi-target bodies (groups, SWAP-shaped
unitaries) surfaced a visual design problem with no clean answer
for the group case in particular. The team's design call:
groups support classical controls only, and the rendering
question for quantum-control connectors on multi-target bodies
isn't worth answering for a scenario the editor doesn't author.
Multi-target unitaries (SWAP) inherit the same refusal as a
structural consequence of the predicate — there is no canonical
attachment point for a single control connector on a body split
across non-adjacent wires.

**What still works.**

- M1's single-target classically-controlled-unitary half is
  preserved. Adding a quantum control to a classically-conditioned
  H gate (one target, no children) still routes through the
  pure-quantum dedup filter from M1 and lands cleanly.
- M2's rendering of pre-existing quantum controls on
  ControlledUnitary-split ops (multi-target unitaries from
  loaded `.qsc` data) is unchanged. The group half of M2 was
  reverted as part of the design lock-in — quantum controls on
  groups no longer render.
- M3's control-leg drag still works on every shape that has
  controls today, because dragging is a permutation of the
  existing control set (rewires leg position) rather than
  creation/destruction — `movingControl` never reaches
  `addControl` / `removeControl`.
- The body-drag path is unaffected. The user can still drag a
  group or SWAP body around as a rigid unit (`_moveAsUnit`).

**What's blocked.**

- Right-clicking the body of a group / SWAP / multi-qubit
  measurement no longer offers "Add Control".
- Right-clicking a group / multi-target op that already has
  controls no longer offers "Remove Control".
- Right-clicking an existing control dot whose parent op is a
  group / multi-target op no longer offers "Remove control"
  (the per-leg menu item is hidden too — the body and the leg
  menus are gated by the same predicate).
- The `movingControl` drag-out-delete gesture (drag a control
  to outside the editor area to remove it) silently no-ops on
  these ops because it routes through `removeControl` under
  the hood. The drag-init still fires but the model is
  unchanged and the next render is a deepEqual no-op.

**Tests.** [circuitActions.test.mjs](../../test/circuit-editor/circuitActions.test.mjs)
pins five contracts:

1. `addControl: refuses on a classically-controlled GROUP`.
2. `addControl: still succeeds on a classically-controlled single-target UNITARY (no children)` — pins the M1 half that survives.
3. `addControl: refuses on a multi-target unitary even without children` — SWAP shape.
4. `addControl: refuses on a plain group (no classical conditions)`.
5. `removeControl: refuses on a multi-target / group op, leaving existing controls in place` — proves loaded-data controls aren't accidentally destroyed by the refused call.

The previously-shipped "addControl on a classically-controlled
GROUP succeeds" test was replaced by item 1 above — its old
positive contract is incompatible with M5 by design.

##### M7: hide Toggle Adjoint on groups — shipped

**What changed.** The context-menu builder in
[contextMenu.ts](editor/contextMenu.ts) omits the "Toggle
Adjoint" entry on every op that has `children` (i.e. every
group). Leaf unitaries are unaffected. Implementation is a
single inline check (`selectedOperation.children == null`) — no
action-layer helper, no walk of the subtree.

**Why this is permanent.** Per the team's design call, groups
are never adjointable through the editor surface. The
underlying considerations:

- A group whose subtree contains a measurement or Reset (ket) is
  not adjointable at all — those operations are irreversible, so
  the group's overall transformation isn't unitary and has no
  mathematical adjoint.
- For groups whose subtree IS adjointable in principle, the
  editor would still need to propagate the group-level
  `isAdjoint` flag into the children for the emitted Q# to
  match the original semantics (reverse child order, apply
  `Adjoint` to each adjointable child, handle the
  `within ... apply` block's own adjoint rule, etc.) — a chunk
  of emitter work that's out of scope for the circuit editor.

Locking the design at "no group adjoint" lets the editor stay
out of the emitter-semantics business entirely.

**What still works.**

- Leaf unitaries (no `children`) always keep "Toggle Adjoint",
  regardless of gate name. Today's behavior unchanged.
- The action / data layer doesn't refuse `isAdjoint = true` on
  groups — this restriction is UI-affordance-only. Loaded
  `.qsc` files with an adjointed group still parse and render
  (the renderer paints the adjoint dagger as before); the user
  just can't toggle the flag from the editor surface. That
  preserves round-tripping for any pre-existing data.

**What's blocked.**

- Right-clicking the body of any group (including pure-unitary
  groups) no longer offers "Toggle Adjoint".

**Tests.** No new tests. The restriction lives entirely in the
context-menu builder (one inline check on `children`); the
action layer is untouched.

#### Known open issues

These have a clear symptom or invariant violation but no fix has
landed. Some are flagged out-of-scope in shipped milestones above.

- **`qubitUseCounts` not refreshed on single-leg control rewire.**
  Flagged out-of-scope by M3. `_moveY`'s leg path doesn't decrement
  / increment per-wire use counts, so `removeTrailingUnusedQubits`
  can underflow in edge cases where the rewired control wire had
  no other uses. Currently invisible because the children's body
  keeps the wire's count above zero; adding a control-only op
  would expose it. Independent of the groups-controls design lock —
  the same code path serves single-target unitaries with controls.
- **Editing / removing the classical condition itself on a
  classically-controlled group.** Out-of-scope flagged by M1 (and
  B1 before it). There is no UI today to convert a
  classically-controlled group back to unconditional, or to swap
  which classical register it depends on. Deferred to the broader
  "editor-authoring" work that also owns B1's architectural fix.
  Independent of the M5 design lock — classical control authoring
  on groups is the surviving authoring surface, just not built out
  yet.

#### Design questions worth re-examining

The earlier draft of this section had a longer list of questions
specifically about authoring quantum controls on groups
(toolbox-drop of a control onto a group, click semantics of a
control dot on a group, visual differentiation between a
group's quantum control and a CNOT control, default-expansion
behavior when adding a control to a group, selecting "the
control on q0 of group X" as a unit). All of those are
**resolved by the design lock-in above**: groups don't carry
editor-authored quantum controls, so there's no gesture, click
target, or selection unit to design.

The questions that survive aren't groups-specific:

- **Should "control" be a first-class toolbox element on
  single-target unitaries?** Currently the only way to add a
  control is to right-click an existing op and use the context
  menu. Making control a draggable toolbox item would close the
  symmetry with gates, but raises new questions about drop
  targets (every single-target unitary would gain
  control-receiving dropzones) and clutter.
- **Keyboard accessibility for add/remove control on
  single-target unitaries.** None of M1–M4 wired keyboard
  equivalents.

#### Roadmap

| Item                                                                                           | Status                                                                                                                                  |
| ---------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| M1: add / remove control plumbing                                                              | ⚠️ Partial — single-target unitary half in effect; group / multi-target half **permanently** reverted by M5 (no group quantum controls) |
| M2: render quantum controls on classically-controlled single-target unitaries (mixed controls) | ✅ Shipped (pending confirm). Group rendering half permanently reverted by the design lock-in.                                          |
| M3: control-leg drag semantics                                                                 | ✅ Shipped (pending confirm)                                                                                                            |
| M4: drag init + ViewState across moves                                                         | ✅ Shipped (pending confirm)                                                                                                            |
| M5: refuse add/remove control on multi-target ops & groups                                     | ✅ Shipped (pending confirm). Permanent design — not gating any future work.                                                            |
| M7: hide Toggle Adjoint on every group                                                         | ✅ Shipped (pending confirm). Permanent design — not gating any future work.                                                            |
| M6: unified quantum-control rendering on multi-target bodies                                   | ✅ Resolved by permanent design — groups never carry editor-authored quantum controls, so no unified renderer rule is needed.           |
| M8: adjoint authoring on groups                                                                | ✅ Resolved by permanent design — groups are never adjointable through the editor surface.                                              |
| `qubitUseCounts` on single-leg rewire                                                          | ❌ Open                                                                                                                                 |
| Classical-condition editing                                                                    | ❌ Deferred (editor-authoring)                                                                                                          |
| Toolbox control element (single-target unitaries)                                              | ❌ Open (design TBD)                                                                                                                    |
| Keyboard accessibility for add/remove control                                                  | ❌ Open                                                                                                                                 |

---

## Bug fixes — open

Bugs discovered in editor flows that don't yet have an owner above.
Tracked separately from the design D-items in the drag-and-drop
section because these are reproducible regressions in shipped
behavior, not in-progress design work.

Listed in rough severity order (crashes first). Each entry has a
"open question" line where the right fix isn't obvious; settle the
question before writing code.

### B1. Classical-control indicators always show `C_null` — ⚠️ partial

**Symptom.** The circle/label next to a classically-controlled
group's control wire reads `C_null` regardless of which classical
register the conditional actually depends on. Should show the
producing register's id (e.g. `C_0`, `C_1`).

**Root cause.** `renderData.classicalControlIds` in
[process.ts](renderer/process.ts) was built solely from
`op.metadata?.controlResultIds` — the global numeric registry the
Rust trace builder populates via [`new_group`](../../../compiler/qsc_circuit/src/builder.rs).
When that metadata is missing (hand-authored `.qsc` files,
programmatically built circuits, future editor-authored classical
controls), the lookup returned `undefined` and the renderer
stringified it as `"null"`.

**Investigation findings.** All 12 trace-built `.qs` snapshots are
clean — the Rust builder always populates `controlResultIds`. The
only failing paths today are `.qsc` files that don't carry the
metadata. The deeper invariant ("every classically-controlled op
must carry `controlResultIds`") is fragile, but only one producer
exists today (the trace builder) and it gets it right.

**Immediate fix (shipped).** Added a fallback in
[process.ts](renderer/process.ts) — when the metadata lookup
misses, use the control register's local `result` field. The label
still renders next to the right wire visually; two M's on
different qubits both displaying `c_0` is acceptable until the
proper global-id story lands.

**Deferred — punted to the future "editor authoring of classical
controls" feature.** When we add a UI path to create a classically-
controlled group from scratch in the editor (currently impossible
— `addControl` only emits pure quantum controls), we'll need to
decide whether to:

- (a) make `controlResultIds` derivable at render time by walking
  the grid once and globally numbering M results in document
  order, making the metadata an optional cache rather than a
  required input, or
- (b) require every producer (trace builder, editor authoring path,
  future tooling) to populate `controlResultIds` and enforce that
  via schema.

Option (a) is the architecturally clean answer; Option (b) keeps
metadata as the source of truth but adds a new invariant to
maintain. Designing this in isolation today is premature; revisit
when the editor-authoring feature gives a second concrete producer
to anchor the design.

### B2. Moving / deleting an M that later gates depend on crashes — ✅ Shipped (pending user-confirmation)

**Symptom.** Drag an M gate that is the producer of a classical
register consumed by a later gate (typically a classically-
controlled group), or delete it via the context menu. The next
render throws.

**Root cause.** Two distinct failure modes shared one underlying
gap — the action layer had no notion that an M's classical
outputs might be referenced elsewhere in the tree.

1. **Delete.** [`removeOperation`](actions/circuitActions.ts)
   removed the M without touching any classically-controlled
   consumer. The consumer's `controls: [{ qubit, result }]`
   still pointed at the deleted producer; the renderer's
   `_getRegY` couldn't resolve the register and threw.

2. **Move.** [`moveOperation`](actions/circuitActions.ts)
   ran the tail-end `_updateMeasurementLines` sweep that
   renumbers per-wire result indices, but the renumber
   propagated only to the M's own `results` array — never to
   downstream classical refs. After a wire change, _two_ Ms
   were involved: the moved M (whose wire and possibly result
   index changed) AND any UNMOVED M on either the source or
   target wire (whose result index got bumped by the renumber
   sweep). Consumers of either could end up with a stale
   `(qubit, result)` key that resolved to nothing.

**Fix.** Cascade pattern, following the `removeQubitLineWithConfirmation`
precedent — action layer stays UI-free, controller layer owns
prompt + render orchestration.

- **Action layer** (`actions/circuitActions.ts`):
  - `collectMeasurementConsumers(grid, mLoc)` walks the tree
    and returns every op whose classical-ref `(qubit, result)`
    matches one of the M's `results` entries.
  - `removeMeasurementWithDependents(model, mLoc, consumers)`
    cascade-deletes consumers first (predicate matches by
    object identity, surviving column splices), then
    re-derives M's location by ref via `_findLocationByRef`
    and calls `removeOperation`. Two-step ordering avoids the
    ancestor-targets refresh seeing dangling refs.
  - `moveMeasurementWithDependents(model, srcLoc, tgtLoc, srcWire, tgtWire, insertCol, invalidatedConsumers)`:
    1. Snapshot every M's pre-move `(qubit, result)` keys by
       object identity (the moved M's clone reference is lost
       through `moveOperation`'s JSON deep-clone, so its
       pre-keys are captured separately).
    2. Call `moveOperation` (move first, then cascade — lets
       us use the pre-move target string against the
       unmodified grid).
    3. Cascade-delete invalidated consumers via
       `findAndRemoveOperations(op => invalidatedSet.has(op))`.
    4. Build `keyRemap: Map<"oldQ:oldR", "newQ:newR">` by
       pairing pre/post snapshots positionally _per M_ — both
       the moved M and any unmoved M whose result indices got
       bumped by the renumber sweep.
    5. Apply remap to every classical ref in the tree via
       `_applyClassicalRefRemap`.
    6. `_deepRefreshDerivedTargets` + `resolveOverlappingOperationsRecursive`
       to handle visual-span changes on surviving classically-
       controlled groups.

- **Controller layer** (`editor/operationPrompts.ts`, new file):
  - `_deleteOperationWithConfirmation(model, loc, renderFn)`:
    non-M / no-consumers path is a direct passthrough; M with
    consumers prompts ("Deleting this M will also delete N
    dependent operation(s)…"), then calls
    `removeMeasurementWithDependents`.
  - `_moveOperationWithConfirmation(model, srcLoc, tgtLoc, srcWire, tgtWire, insertCol, renderFn)`:
    non-M / no-consumers path is a direct passthrough; M with
    consumers partitions via `Location.inEarlierColumnThan(targetLoc, consumerLoc)`
    into survivors (consumer still strictly after M's new
    column → classical-ref remap) and invalidated (consumer
    at-or-before → cascade-delete), prompts with a
    three-shape message (pure-survivors / pure-invalidated /
    mixed), then calls `moveMeasurementWithDependents`.

- **Wiring**: context menu Delete and `dragController`'s
  drag-out-delete + drop-commit-move all route through the
  controller wrappers. `mutationHandledByWrapper` local in
  `onDropzoneMouseUp` gates the trailing render to avoid
  double-renders.

- **Ctrl+drag**: clone path is unchanged. The new wrappers
  only run for source-side mutations; clone synthesizes a
  fresh op and never touches the source M's consumers.

**Open question (resolved).** Cascade-delete on invalidate,
remap on survive. The "do a mix" answer to the column-order
edge case is implemented by partitioning the consumer set
and surfacing both counts in the prompt message.

**Tests.** 10 new cases in
[`circuitActions.test.mjs`](../../test/circuit-editor/circuitActions.test.mjs)
covering `collectMeasurementConsumers` (empty, top-level,
nested, classical-ref must match), `removeMeasurementWithDependents`
(cascade + location re-derive after collapse), and
`moveMeasurementWithDependents` (survivor remap, invalidated
cascade, the unmoved-M renumbering propagation case, and the
no-consumer passthrough sanity check). 382 → 392 tests pass.

**Subsumes.** B4 — orphaned classical refs no longer exist in
the model post-fix.

### B3. Moving qubits around an M that later gates depend on crashes — ✅ Shipped (user-confirmed)

**Symptom.** Reorder qubit wires (drag a qubit label up or
down) such that an M's producer wire ends up after a consumer's
column. Crash on next render.

**Original diagnosis (turned out to be wrong).** Same family as
B2 — column-order invariant ("producer's column strictly
precedes consumer's column") violated by the wire reorder.
Proposed fix was to run D2's column-order validation inside
`moveQubit`.

**Actual root cause.** Same family as B2, but the
column-order story doesn't apply: `moveQubit` is a **1-to-1
wire permutation** that doesn't touch column positions at all,
only wire indices. The crash was identical in shape to B2's —
the renderer's `_getRegY` chasing a `(qubit, result)` key
that no longer resolved — but the trigger was different: any
register reference whose `qubit` field went stale because the
wire-index remap missed it.

**Fix (came for free).** Two pieces, both already in place by
the time this bug was retested:

1. `moveQubit`'s [`remapRefsInGrid`](actions/circuitActions.ts)
   walks every op in every column at every nesting depth and
   feeds its registers through the wire-permutation function —
   including M's `.qubits`, M's `.results`, every consumer's
   `.controls`, every nested group's eager `.targets` cache,
   and refs buried inside collapsed children. The 1-to-1
   permutation preserves uniqueness as long as the pre-state
   was well-formed: no orphans, no duplicate `(qubit, result)`
   keys.

2. The [`_applyClassicalRefRemap`](actions/circuitActions.ts)
   fix (skip M's producer-side `.results` when applying the
   consumer remap) — landed alongside B2 — closed the
   collateral double-remap path that could corrupt M.results
   under certain `moveMeasurementWithDependents` flows. Not
   reached by `moveQubit` itself but shared the same renderer
   crash surface.

**Regression coverage.** Five `moveQubit`-with-classical-
consumer tests in
[circuitActions.test.mjs](../../test/circuit-editor/circuitActions.test.mjs):
single-M wire swap, two-Ms-on-different-wires swap, multiple-Ms-
on-same-wire swap, `isBetween` move past a wire with multiple
Ms-with-consumers, and a buried-in-nested-groups consumer.
A header-comment block in the test file documents why
`moveQubit` is structurally immune to the double-remap class
of bug (it doesn't call `_updateMeasurementLines`, so no
producer values are recomputed beneath a consumer remap).

### B4. Removing an M doesn't update later classical wire positions

inside collapsed groups — ✅ Subsumed by B2 (pending user-confirmation)

**Symptom.** Delete an M whose results are referenced by gates
inside a later collapsed group. The visible classical sub-wire
positions on subsequent qubits stay where they were, leaving
ghost gaps or misaligned wires until something forces a full
re-layout.

**Likely cause.** When an M is removed, the producing qubit's
classical register count should drop — but the consumers inside
the collapsed group still hold references and the row-height
computation (in [sqore.ts](sqore.ts)'s `getRowHeights`) doesn't
re-walk the collapsed children to discover the now-orphaned
classical refs. The view caches stale heights.

**Fix.** B2's cascade-delete pass eliminates the orphaned-ref
state entirely — by the time `removeOperation` runs, every
consumer of the deleted M is already gone, so the row-height
pass has nothing stale to count. The "fix at the row-height
layer" alternative (walk collapsed children) is no longer
needed.

**Open question (resolved).** Data-layer fix (B2's cascade)
was the right call — orphaned classical refs shouldn't exist
in the model at all.

### B5. Add / remove control on a classically-controlled op blocked by classical-ref entries — ✅ Shipped (pending user-confirmation)

**Symptom.** A classically-controlled group consumes an M on
qubit Y. Try to add or remove a quantum control on Y (or, for
groups, on any wire — the group's `.targets` carried a
classical-ref entry on Y that fooled the dropzone visibility
check). The context-menu "Add control" wire-pick either showed
no dropzone on Y, or the action silently failed because
`addControl`'s dedup matched the classical-ref entry.

**Root cause.** A classically-conditional op records its
classical-register dependency as a `{qubit: Y, result: N}` entry
in BOTH `.controls` (the conditional dependency) AND `.targets`
(the visual-extent claim that draws the gate down to the
classical-register box on Y). Four sites treated `.controls` /
`.targets` as flat qubit-only lists, with no distinction between
pure-quantum entries (`result === undefined`) and classical refs:

1. [`startAddingControl`](editor/controllers/dragController.ts)'s
   dropzone-visibility filter — `isTarget` / `isControl` matched
   by `qubit` only, so wire Y was treated as "already a target"
   and got no dropzone.
2. [`startRemovingControl`](editor/controllers/dragController.ts)'s
   `.controls?.forEach` — created a remove-control dropzone for
   the classical-ref entry too, even though it has no control-
   dot to click and shouldn't be removable as a quantum control.
3. [`addControl`](actions/circuitActions.ts)'s `existingControl`
   lookup — a classical-ref `{qubit: Y, result: N}` blocked
   adding a new pure-quantum control on Y (returned false).
4. [`removeControl`](actions/circuitActions.ts)'s `findIndex`
   lookup — would have matched the classical-ref entry instead
   of (or in addition to) the quantum entry on Y.

**Fix.** All four sites now filter for pure-quantum entries
(`result === undefined`), following the same pattern
[`getQuantumWireRange`](utils.ts) and the use-count helpers
([`CircuitModel.incrementQubitUseCountForOp`](data/circuitModel.ts))
established for editor-scope decisions.

**Out of scope (deferred to editor-authoring feature).**
Removing / editing the classical condition itself — "convert
classically-controlled to unconditional" — is a separate
semantic from "remove a quantum control". B5 only addresses the
quantum-control half; classical-condition editing waits for the
broader editor-authoring work that B1 also defers to.

**Tests.** Five new tests in
[test/circuit-editor/circuitActions.test.mjs](../../test/circuit-editor/circuitActions.test.mjs):
add quantum control on the classical-owner wire of a conditional
gate succeeds (both entries survive in `.controls`); legacy
duplicate-quantum-control dedup still returns false; remove
quantum control on a wire that also has a classical-ref leaves
the classical ref intact; remove on a wire that ONLY has a
classical-ref returns false (no-op); add quantum control on the
M-owner wire of a classically-controlled GROUP succeeds.

### B6. Shift-extend onto / past an external sibling — ✅ Shipped (pending user-confirmation)

**Symptom.** Shift-drag a group's expand chevron downward to
extend the group's wire span. Two related problems:

1. The user could drop on a wire ALREADY occupied by an
   external sibling of the group in the same outer column —
   violating the general "you can't drop a gate directly onto
   another gate" rule that all other dropzones honor.
2. When the extension crossed PAST an external sibling on an
   in-between wire to a clear drop wire, the in-between sibling
   was not always horizontally shifted out of the way — so the
   widened group ended up visually overlapping it.

**Root cause #1 (no dropzone filter, AT ANY level).**
[`spawnShiftExtendDropzones`](editor/controllers/dragController.ts)
emitted a dropzone at every (inner column, outer-wire) pair
outside the parent group's current span, regardless of whether
that outer wire was already occupied by an external sibling in
the parent's outer column — or, more subtly, in the outer column
of ANY ancestor up the chain. No equivalent of the
`isTarget`/`isControl` filter that
[`startAddingControl`](editor/controllers/dragController.ts)
already does for its own dropzone set. The first shipped pass
filtered only the immediate parent's column; a user-reported
follow-up revealed the chain-walk dimension and the helper was
generalized to `getAncestorColumnSiblingWires`.

**Root cause #2 (cascade SHALLOW case was fine; deep case had
a silent early-exit bug).** Tracing the action layer initially
suggested the cross-over case was already handled by the
existing dest-side cascade: `moveOperation`'s
[`refreshAncestorTargets(destAncestorChain, { onAfterRefresh: ... })`](actions/circuitActions.ts)
runs `_resolveOverlapAfterExtend(parentOp, parentContainingArray)`
on the widened parent group, which splits the outer column so
the in-between sibling slides one column to the right of the
parent — identical to the [`commitAddControl`](editor/controllers/dragController.ts)
pattern for "add a control widens an op". For _shallow_
shift-extends (immediate-child source), the dest ancestor
chain has exactly one rung and `onAfterRefresh` fires before
the early-exit check, so the resolver runs and the column
splits as expected. **That's why the first round of B6 tests
all passed.**

A user-reported follow-up exposed the _deep_ case: when the
shift-extend source is a grandchild (or deeper) of the parent
group, the source-side cascade
([`refreshAncestorTargets(survivedSourceChain)`](actions/circuitActions.ts),
no hook) runs FIRST and propagates the new wire span up
through every shared ancestor. By the time the dest-side
cascade runs, `_refreshDerivedTargets` on the innermost rung
returns `false` (targets already correct), and the original
`if (!changed) return;` exited the whole walk — never firing
`onAfterRefresh` on the _higher_ rungs where the collision
actually lives. The widened top-level ancestor remained in the
same column as its sibling; the rendered expanded view appeared
to enclose the sibling even though the data model still treated
it as external.

The doc comment on
[`refreshAncestorTargets`](actions/circuitActions.ts) already
spelled out the intended contract — _"the hook fires on every
visited still-attached rung, regardless of whether the refresh
changed `.targets`"_ — but the implementation contradicted it.
Doc and code were out of sync; the fix makes the code match
the doc.

**Fix.**

1. New [`getOuterColumnSiblingWires`](utils.ts) helper computes
   the set of quantum wires occupied by external siblings of an
   op in its outer column. Classical-ref entries
   (`result !== undefined`) are excluded via
   [`getQuantumWireRange`](utils.ts) — they paint as thin
   indicators, not real wire occupants. Mirrors B5's
   `result === undefined` filter pattern.

2. New [`getAncestorColumnSiblingWires`](utils.ts) composes the
   single-level helper across the FULL ancestor chain via
   [`Location.parent()`](data/location.ts). The shift-extend
   gesture extends the immediately enclosing group, but the
   action-layer cascade
   ([`refreshAncestorTargets`](actions/circuitActions.ts) +
   [`_resolveOverlapAfterExtend`](actions/circuitActions.ts))
   widens EVERY ancestor whose span doesn't already enclose the
   drop wire — and each widening can collide with siblings IN
   THAT ANCESTOR'S column. Filtering only the immediate parent
   under-blocks: a deeply nested source can be shift-dropped
   onto a wire owned by a top-level sibling of an ancestor
   several levels up, leaving the cascade to silently cope.

   _Why over-blocking is impossible._ If ancestor A's span
   already encloses the drop wire `w`, A doesn't widen and we
   wouldn't need to consult A's siblings — but A's siblings
   are by column invariant guaranteed not to be on `w` in that
   case (they share A's column, so they're outside A's span).
   Including them in the union is therefore redundant but never
   produces a false positive.

3. `spawnShiftExtendDropzones` consults
   `getAncestorColumnSiblingWires` and skips any wire in the
   blocked set. The CROSS-OVER case (extending PAST a sibling
   to a clear wire) stays allowed — that's the whole point of
   the cascade splitting the column when needed.

4. [`refreshAncestorTargets`](actions/circuitActions.ts)
   no longer early-exits when an `onAfterRefresh` hook is
   provided. The early-exit condition is now
   `if (!changed && options.onAfterRefresh == null) return;`
   — the refresh-only optimization stays in place for hookless
   callers (source-side cascade, `removeOperation`,
   `removeControl`), but any caller registering a side-effect
   hook walks the full chain. This makes the implementation
   match the doc-comment contract and fixes the deeply-nested
   cross-over case without affecting shallow scenarios. Cost
   is bounded by ancestor depth (typically ≤4) and the per-rung
   `_resolveOverlapAfterExtend` is a no-op when the rung's
   column has no siblings or no overlap.

**Tests.** Fourteen new cases:

- Eight in [`utils.test.mjs`](../../test/circuit-editor/utils.test.mjs):
  - Six for `getOuterColumnSiblingWires` directly: null/empty
    location, no siblings, multi-sibling enumeration, multi-wire
    siblings (groups / SWAP-shaped ops) expand to a wire RANGE,
    classical-ref entries skipped, other-column ops not counted,
    nested ops use their OWN containing grid.
  - Two more for the same helper: location resolving to a
    missing parent array returns empty.
  - Five for `getAncestorColumnSiblingWires`: null/empty
    location, top-level location reduces to the single-level
    helper (chain of length 1), deeply-nested location unions
    sibling wires from EVERY level, classical-ref exclusion
    propagates through the chain, unresolved location returns
    empty.
- Three in [`circuitActions.test.mjs`](../../test/circuit-editor/circuitActions.test.mjs):
  - Two pinning the cross-over cascade for shapes the earlier
    D4-Stage-B test coverage didn't reach (shallow case): a
    GROUP sibling (multi-wire, with children) splits the outer
    column with both groups intact; an in-between-wire sibling
    with a clear drop wire still triggers the split (resolver
    shifts COLUMNS, not WIRES).
  - One pinning the _deep_ case the early-exit fix unblocks:
    3-level nesting (Outer > Middle > Foo > leaves) with a
    multi-wire group sibling at the top level. Without the fix,
    the dest-side cascade returns at the innermost rung and the
    top-level column never splits; with the fix, the walk
    reaches Outer and the resolver splits the column.

**Out of scope.**

- Vertical pushing (moving siblings to a different WIRE to make
  room for the extension) is not implemented — the resolver
  shifts horizontally, matching every other "drop widens an
  op" path in the editor. The original B6 wording suggested
  "pushed down" but the user clarification ("shifted
  horizontally so they don't overlap vertically") confirms
  horizontal is the right axis.
- Cross-column displacements (extension into wires owned by
  ops in DIFFERENT outer columns than the group): same axis —
  different outer columns can never visually overlap, so no
  resolution is needed. The original B6 "fix direction" note
  about checking every column was based on a misreading of the
  layout model.

**Follow-up correction (PR-prep review) — ✅ shipped (pending user-confirmation).**
The B6 fix above blocked siblings using
[`getQuantumWireRange`](utils.ts) on the premise that
classical-ref entries "paint as thin indicators, not real wire
occupants." A pre-PR walkthrough of the test for
[`getOuterColumnSiblingWires`](utils.ts) revealed that's wrong
about the GEOMETRY: a classically-controlled sibling `Z @ q3
cref q0r0` doesn't just paint at q3 — its connector descends
through every wire between q3's body and the q0r0 classical
row (which sits BELOW q0, between q0 and q1). So the sibling
visually covers q1, q2, q3; q0 is above the connector's
endpoint and is NOT covered. The old quantum-only filter
under-blocked (only {q3}); naively widening to all referenced
qubits would over-block (would include q0).

Fix: new [`getWireRange(op)`](utils.ts) helper returning
`[Register, Register]` endpoints in the half-step ordering
(classical row immediately below its owning qubit row). Scope
deliberately kept tight — only
[`getOuterColumnSiblingWires`](utils.ts) was switched over;
the other call sites of `getQuantumWireRange` / `getMinMaxRegIdx`
were left alone to be audited as a separate item (see
"Wire-range helper consolidation — deferred" below). 7 new
`getWireRange` tests in
[utils.test.mjs](../../test/circuit-editor/utils.test.mjs); the
2 existing classical-ref tests for
`getOuterColumnSiblingWires` / `getAncestorColumnSiblingWires`
were updated to assert the new geometric coverage. 419/419 in
the circuit-editor suite (was 412 before this correction).

### Wire-range helper consolidation — deferred

`utils.ts` now has three close-but-not-identical helpers for
"what wires does this op touch":

- [`getMinMaxRegIdx`](utils.ts) — every register; treats
  classical-ref `.qubit` as if it were a regular qubit row
  (off-by-one in the over-blocking direction). Used by
  cascade overlap in [circuitActions.ts](actions/circuitActions.ts)
  via `_getMinMaxRegIdx`.
- [`getQuantumWireRange`](utils.ts) — only registers with
  `result === undefined`. Used by
  [dragController.ts](editor/dragController.ts) (multi-target
  drag legs, shift-extend reach) and
  [draggable.ts](editor/draggable.ts) (dropzone occupancy,
  child window for groups).
- [`getWireRange`](utils.ts) — geometric endpoints as
  `Register`s, half-step ordering (added in the B6 follow-up
  above). Used only by
  [`getOuterColumnSiblingWires`](utils.ts).

Each helper answers a subtly different question; some have
policy baked in (which connector wires "count" as occupancy),
which is the part that's currently inconsistent across call
sites. The fully consolidated end-state is a small set of
geometry-only helpers (no policy), with each call site
documenting its own policy at the use site. Estimated audit
load: 5 sites that probably want `getWireRange` (one in
[utils.ts](utils.ts) already done; the 5 in
[circuitActions.ts](actions/circuitActions.ts) and
[dragController.ts](editor/dragController.ts) line 815 are
the likely targets) and 2 sites that are intentional policy
carve-outs and should stay on `getQuantumWireRange`
([draggable.ts](editor/draggable.ts) lines 557 and 640).
**Not in this PR** — the cascade in `circuitActions.ts` is a
hot path; each swap needs its own regression-test plan.

### `findAndRemoveOperations` should be action-layer internal — deferred

Found while moving the action-layer tests into
[circuit-actions/addRemove.test.mjs](../../test/circuit-editor/circuit-actions/addRemove.test.mjs).

[`findAndRemoveOperations`](actions/circuitActions.ts) is
exported from the action-layer API but has a non-obvious
contract: it decrements `qubitUseCounts` and prunes empty
columns, but does **not** trim trailing unused wires — every
other `remove*` action does. The asymmetry is load-bearing for
the only external caller
([qubitController.ts](editor/controllers/qubitController.ts)
line 67), which depends on the wire count staying stable
between the cascade and the immediately-following `removeQubit`
call (an eager trim would invalidate `qubitIdx`). The two
in-action-layer callers
([`moveMeasurementWithDependents`](actions/circuitActions.ts)
line 899 and
[`removeMeasurementWithDependents`](actions/circuitActions.ts)
line 1038) each follow it with a trimming `remove*` of their
own, so the no-trim contract is also correct for them — but
incidentally rather than by design.

The proper shape is the same `*WithDependents` pattern those
two callers already follow: introduce a public
`removeQubitWithDependents(model, qubitIdx)` that owns the
cascade + `removeQubit` sequence, then make
`findAndRemoveOperations` module-internal
(`_findAndRemoveOperations`). The "callers must trim" footgun
disappears from the public surface;
[qubitController.ts](editor/controllers/qubitController.ts)
collapses to a single call and stops needing the
`getOperationRegisters` import. Test coverage migrates from
direct primitive calls to the new public action, which is the
honest framing — if the function is no longer public, its
tests shouldn't be either.

**Not in this PR** — production-code refactor, separate
concern from the test reorganization that surfaced it. Should
land alongside the next action-layer cleanup pass.

### B7. Qubit rearrangement doesn't update group contents correctly — ✅ shipped

**Symptom.** Drag a qubit label to reorder wires. Ops whose
references should follow their wire end up referencing the new
wire index in the wrong way — e.g. an op inside a group that
previously addressed wire 2 now silently addresses wire 3, or
the group's `.targets` cache doesn't get refreshed.

**Root cause.** `moveQubit` in
[circuitActions.ts](actions/circuitActions.ts) was iterating only
`model.componentGrid`'s top-level columns. Two consequences:

1. Wire-index remap never reached ops nested inside group
   `children`. Those ops kept their old wire indices and the
   renderer drew them on the wrong rows.
2. The post-mutation pass to refresh group `.targets` caches and
   resolve any newly-introduced overlaps only ran at the top
   level. Group `.targets` arrays touched by the in-place remap
   went stale (the remap could introduce duplicate refs, lose
   ordering, or both); sibling-column collisions inside groups
   went unresolved.

Compare `removeQubit` in the same file, which already had a
recursive `shiftRefsInGrid` helper — a uniform shift kept caches
in lockstep without needing a refresh, but the precedent for
walking children was already there.

**Fix (shipped).** Three changes in
[circuitActions.ts](actions/circuitActions.ts):

1. Extracted the wire-index remap into a pure `remapWire(old)`
   function, then applied it via a recursive `remapRefsInGrid`
   helper that walks `op.children` — same shape as
   `removeQubit`'s `shiftRefsInGrid`. Group ops' own cached
   `.targets` / `.results` arrays are remapped in-place as part
   of the walk (they're independent `Register` objects, not
   shared references with descendants).
2. Followed the remap with a call to
   [`_deepRefreshDerivedTargets`](actions/circuitActions.ts) to
   re-derive every group's `.targets` from its children's
   already-correct caches. The remap can both widen AND narrow
   group spans (the existing `_deepRefreshDerivedTargets` doc
   notes "narrowing-only" but it actually re-derives from
   immediate children either way; the contract note refers to
   not running the overlap resolver, which we handle separately
   in step 3).
3. Added a new private
   `resolveOverlappingOperationsRecursive(grid)` that calls
   the existing single-grid resolver at every nesting level.
   Replaces the old top-level-only
   `resolveOverlappingOperations(model.componentGrid)` call.

**Tests.** 3 new tests in
[test/circuit-editor/circuitActions.test.mjs](../../test/circuit-editor/circuitActions.test.mjs)
under "moveQubit: recurses into nested groups":

- swap wires propagates into nested op `.targets`
- swap wires refreshes the parent group's cached `.targets`
- swap wires doesn't corrupt nested children when no overlap is
  introduced

343/343 tests pass (was 340 before B7).

**Out of scope / deferred.** Validation that the remap doesn't
violate classical-register dependencies (producer M must remain
in an earlier column than its consumer) is covered separately by
B3. B7 is purely about getting the data structure right after
the move; B3 will add the refusal check that prevents reorders
from putting the model into an unrenderable state in the first
place.

### B8. Clone-move of a multi-wire group rewrites `.targets` to a single-wire stub — ✅ Shipped (pending user-confirmation)

**Symptom.** Ctrl+drag (clone) of a multi-wire group dropped the
copy with a single-wire `.targets` (just `[{qubit: targetWire}]`)
and stranded the nested children on their original wires. Visible
worst on split groups grabbed from the top-most box, but the
underlying bug affected every group / multi-target clone.

**Root cause.** `addOperation` in
[circuitActions.ts](actions/circuitActions.ts) unconditionally
rewrote `.targets` (or `.qubits` for measurements) to
`[{qubit: targetWire}]` on the deep-copied source. This is fine
for toolbox drops (single-target templates) but wrong for any op
where `_moveAsUnit` returns true — groups and multi-target /
multi-qubit gates. `moveOperation`'s unit-move path shifts every
register by the same delta; the clone path had no equivalent.

**Fix.** Extended `addOperation` with an optional `sourceWire`
parameter. When provided AND `_moveAsUnit(op, false)` returns
true, the deep-copied subtree is shifted by
`targetWire - sourceWire` via the same `_shiftAllRegisters`
helper `_moveY` uses on its unit branch — including internal
classical-register lockstep handling. The drag controller's
clone path in
[dragController.ts](editor/controllers/dragController.ts) now
passes `selectedWire` as `sourceWire`. Toolbox drops continue to
omit it and keep the legacy single-leg rewrite (no regression).

Underflow protection (refuse the clone if any wire would shift
below 0) and post-add `_updateMeasurementLines` refresh for every
wire the cloned subtree touched both mirror `moveOperation`'s
unit-move tail.

**Tests.** Six new tests in
[test/circuit-editor/circuitActions.test.mjs](../../test/circuit-editor/circuitActions.test.mjs):
group clone with delta>0, group clone with delta=0, multi-target
(SWAP) clone, group-with-internal-classical-control clone (the
classical ref shifts in lockstep with the cloned producer),
underflow refusal, and legacy single-leg behavior preserved when
`sourceWire` is omitted.

### B9. Quantum controls on a group are never drawn — ✅ Shipped (pending user-confirmation)

**Symptom.** Adding a pure-quantum control to a group via the
context-menu "Add Control" wire-pick mutates the model correctly
(`op.controls` gets the new entry; B5 ensures the
dedup/lookup behave right) but produces no visible change. No
control dot, no connector — the dashed box looks identical
before and after.

A second variant, surfaced after the initial fix: adding a
quantum control to a CLASSICALLY-CONTROLLED group (the M-owner
case from B5) drew the new control as a stray classical-circle
"c_null" indicator on the qubit wire, instead of as a standard
control dot. Same root cause as B1 surfacing through a
per-control rendering bug rather than a missing-id bug.

**Root cause.** Two layers.

1. [`_groupedOperations`](renderer/formatters/gateFormatter.ts)
   rendered the dashed box, the children, and the label, but
   never looked at `renderData.controlsY` for the pure-quantum
   case. The classically-controlled branch had its own dedicated
   `_classicalControls` path; the quantum-only path simply had
   no render code. So adding a quantum control to a pure-quantum
   group was a silently-rendered no-op.

2. `_classicalControls` iterated EVERY entry in `controlsY` and
   drew it as a classical dashed-circle indicator, regardless of
   whether the underlying control register was a classical ref
   (`{qubit, result}`) or a pure-quantum control (`{qubit}`).
   For a classically-controlled group with a mixed quantum
   control (post-B5), the quantum control wire got a "c_null"
   circle slapped on it — visually wrong, conceptually wrong.

**Fix.** Three coordinated changes.

1. [`process.ts`](renderer/process.ts) `classicalControlIds` now
   returns `undefined` for quantum-control entries (was `null`).
   `null` stays reserved for classical refs whose id couldn't be
   resolved (the B1 case). The `(number | null | undefined)[]`
   type makes the three-way distinction explicit.

2. [`_classicalControls`](renderer/formatters/gateFormatter.ts)
   skips entries where the id is `undefined`. Per-control
   routing is now driven by per-control type rather than the
   op's overall control flavor.

3. New
   [`_renderQuantumGroupControls`](renderer/formatters/gateFormatter.ts)
   helper emits a `controlDot` on each quantum control wire (at
   the group's center x) plus a thin `control-line` connector
   to the nearest dashed-box edge — top edge if above, bottom
   edge if below, no connector if the wire crosses the box.
   `_getQuantumControlYs` filters `controlsY` to just the
   quantum entries (using `classicalControlIds[i] === undefined`).
   Wired into both branches of `_groupedOperations` and into the
   `GateType.Unitary` case in `formatGate` so a classically-
   controlled single-op Unitary with a mixed quantum control
   also renders correctly.

The expanded-group dashed-box geometry is now always derived
from `targetsY` alone — which already includes classical-control
wires via the `_opToRenderData` patch — so the box still extends
to the classical wire (needed for `_classicalControls` to attach
its dashed connector) but never extends to a pure-quantum
control wire (the design call "dots outside, box unchanged"
holds for pure, classical-only, and mixed cases uniformly).

**Out of scope.** None — mixed controls now render correctly
end-to-end. The editor-authoring feature (B1's deferred
architectural work) remains the home for editing/removing the
classical condition itself.

**Tests.** Three snapshot cases in
[test/circuits-cases/](../../test/circuits-cases/):
[quantum-control-group.qsc](../../test/circuits-cases/quantum-control-group.qsc)
covers expanded pure-quantum-control groups with
control-above, control-below, and control-inside-body scenarios.
[quantum-control-group-collapsed.qsc](../../test/circuits-cases/quantum-control-group-collapsed.qsc)
documents the collapsed-branch input (the harness force-expands
via `renderDepth`, so this exercises the same code paths as the
expanded fixtures but with a single-summary-box layout shape).
[quantum-control-classical-group.qsc](../../test/circuits-cases/quantum-control-classical-group.qsc)
covers the mixed-controls regression: a classically-controlled
group with an additional quantum control renders the dashed
`c_0` circle on the classical wire AND a solid control dot on
the quantum wire — not two classical circles.

### B10. Control drag on a group moves the whole group instead of just the control — ✅ Shipped (pending user-confirmation)

**Symptom.** With B5+B9 shipped, controls on groups now exist
and render correctly, but interacting with them feels broken:

- Dragging a control vertically (e.g. from q0 to q3) shifts the
  ENTIRE group up by 3 wires instead of just rewiring the control.
- Dropping a control onto a body wire of the group doesn't swap
  them — the body just slides out of place.

The horizontal-drag and like-register-guard behaviors that work
correctly for non-group ops (CNOT-style 1 target + N controls)
all silently degrade for groups.

**Root cause.**
[`_moveAsUnit`](actions/circuitActions.ts) decided the move
strategy by checking `op.children != null` BEFORE checking
`movingControl`. Group + movingControl therefore short-circuited
into the unit-shift path (`_shiftAllRegisters` with delta =
`targetWire - sourceWire`), which is correct for whole-group
relocation but completely wrong for the "drag a single leg" intent
the control-dot grab signals.

**Fix.** Three coordinated changes in
[circuitActions.ts](actions/circuitActions.ts):

1. `_moveAsUnit` now checks `movingControl` first. A control move
   on any op — group or not — falls through to the single-leg
   path, matching the long-established CNOT-style "drag a control
   to rewire just that leg" interaction.

2. New `_swapWiresInSubtree` helper recursively swaps every
   register reference on `wireA` with every reference on `wireB`
   throughout an op's subtree. Used by the group + control branch
   below to implement the "drop control onto body wire to swap"
   gesture.

3. `_moveY`'s `unitary + movingControl` branch now detects when
   the group's body occupies the drop wire (`groupBodyIncludesTargetWire`,
   captured BEFORE the `unlikeRegisters` mutation that would
   otherwise hide it in the derived `.targets` cache). When true,
   it walks the children and swaps source ↔ target; either way it
   rewires the control to `targetWire`, then re-derives the
   moved group's own `.targets` from its (possibly-swapped)
   children via `_refreshDerivedTargets`.

The horizontal-drag case "just worked" once `_moveAsUnit` was
fixed: `_moveY` with sourceWire === targetWire is a no-op via the
like-register guard (delta = 0, control already on targetWire),
and `_moveX` handles the column relocation independently.

**Tests.** Four new cases in
[circuitActions.test.mjs](../../test/circuit-editor/circuitActions.test.mjs):
vertical control drag on a group rewires only the control;
dropping a control onto a body wire swaps with that body wire and
re-derives `.targets`; dropping onto a wire already occupied by
another control is a no-op (like-register guard still applies);
horizontal control drag moves the whole op to the new column.

**Out of scope.** `qubitUseCounts` per-wire bookkeeping isn't
updated by the single-leg control-rewire path — pre-existing
limitation shared with the non-group CNOT case. The model still
renders correctly because `removeTrailingUnusedQubits` is the
only consumer of stale-low counts, and the wire counts don't
underflow from a control-only move (the wire still has uses
from the children body).

### B11. Control drag on a group: expanded groups blocked + classically-controlled groups re-expand on every move — ✅ Shipped (pending user-confirmation)

**Symptoms.** Two interaction bugs surfaced once B10 made
controls on groups draggable:

1. **Expanded groups can't be control-dragged at all.** Clicking
   the control dot on an expanded group did nothing — the drag
   never started. Collapsed groups worked fine.
2. **Classically-controlled groups always re-expand when their
   control is moved.** Even a same-wire (no-op) horizontal control
   drag silently re-expanded a group the user had explicitly
   collapsed.

**Root cause #1 (drag-blocking).**
[`DragController.onGateMouseDown`](editor/controllers/dragController.ts)
early-returned without setting `selectedOperation` whenever the
gate elem had `data-expanded="true"` — the intent being to
prevent the user from grabbing an expanded group as a whole. But
an expanded group's control dots are DIRECT children of its outer
`<g class="gate" data-expanded="true">` node (children's gate
wrappers catch their own events and stop propagation; only the
top-level controls bubble up). The control-dot mousedown went to
`selectionController` first (which set `movingControl = true`),
then bubbled to the gate handler, which short-circuited before
resolving `selectedOperation`. The null check three lines later
exited and no drag ever started.

**Root cause #2 (ViewState lost).**
[`moveOperation`](actions/circuitActions.ts) deep-clones the
source op via `JSON.parse(JSON.stringify(...))` (line 142) so the
new op has a different object identity than the original.
[`Sqore.rebaseViewState`](../sqore.ts) is identity-keyed: it
walks `lastLocationMap` (`Map<Operation, string>`) and looks up
each op in the post-mutation `next` map by object reference. The
deep clone isn't found in `next` → maps to `null` → ViewState
entry dropped. On the next render, defaults kick back in; the
`hasClassicalControls && hasChildren` default-expansion rule in
[`process.ts`](renderer/process.ts) re-expands the group. Pure
quantum groups have the bug too but it's invisible because their
default IS collapsed.

**Fixes.** Two coordinated changes:

1. **`DragController.onGateMouseDown`** now extends the
   selectedOperation-resolution condition to also run when
   `this.ctx.interaction.movingControl === true`. The
   `movingControl` flag is only set by `selectionController` when
   the mousedown host was a `.control-dot`, so the carve-out is
   precise: ordinary clicks on an expanded group's dashed box
   still no-op (preserving the "no grabbing expanded groups as a
   whole" behavior), but clicks on its direct-child control dots
   correctly resolve the group as the drag target.

2. **`moveOperation` + `Sqore.rebaseViewState` cooperate via a
   one-shot stamp.** After the deep-clone, `moveOperation` writes
   `dataAttributes["sqore-prev-location"] = sourceLocation` on
   the new op. `rebaseViewState` builds a
   (prev-location → new-location) fallback map from any ops
   carrying the stamp, consumes (deletes) each stamp on read, and
   falls back to that map when the identity lookup misses. The
   stamp is gone before the next render's deep-copy step, so it
   never leaks into the rendered SVG as a `data-*` attribute or
   accumulates across edits.

**Why the stamp instead of preserving identity in
`moveOperation`?** A no-clone refactor of `moveOperation` would
have wider blast radius — `_addOp` is shared with `_moveX` and
expects the source it inserts is a separate reference from the
one `_removeOp` later strips out (the "removed" marker pattern
relies on this). The stamp is a minimal, surgical contract change
that keeps `moveOperation`'s deep-clone invariant intact.

**Tests.**
[`dragController.test.mjs`](../../test/circuit-editor/dragController.test.mjs)
gains two cases:

- `onGateMouseDown` on an expanded group's control dot DOES set
  `selectedOperation` when `movingControl` is true (the new
  carve-out path).
- `onGateMouseDown` on an expanded group WITHOUT `movingControl`
  still no-ops (no regression on the original
  "can't grab an expanded group" guard).

[`circuitActions.test.mjs`](../../test/circuit-editor/circuitActions.test.mjs)
gains three cases pinning the stamp contract:

- A plain `moveOperation` stamps `sqore-prev-location` on the
  returned op with the pre-move source location.
- The stamp is set even when the source op had no
  `dataAttributes` going in (lazy-create path).
- The stamp is set on the control-leg move on a group (the exact
  scenario that triggered B11 in the wild).

### Roadmap & status

> B5, B9, B10, B11 are also milestones M1–M4 in the
> [Controls on groups](#controls-on-groups--feature) feature
> section above. The detailed bug entries below remain as
> engineering archaeology; the feature section is the right
> place to scope further work in that area.

| Item                                                             | Severity         | Status                                                                                              |
| ---------------------------------------------------------------- | ---------------- | --------------------------------------------------------------------------------------------------- |
| B1: classical-control indicators show `C_null`                   | Display bug      | ⚠️ Partial (immediate symptom fixed; architectural fix deferred to future editor-authoring feature) |
| B2: moving / deleting M with downstream deps crashes             | Crash            | ❌ Open                                                                                             |
| B3: qubit reorder around dependent M crashes                     | Crash            | ❌ Open                                                                                             |
| B4: M removal leaves stale classical wire layout                 | Layout bug       | ❌ Open                                                                                             |
| B5: add/remove control fails on classical groups _(M1)_          | Logic error      | ✅ Shipped (pending user-confirmation)                                                              |
| B6: shift-extend onto / past an external sibling                 | Layout bug       | ✅ Shipped (pending user-confirmation)                                                              |
| B7: qubit reorder doesn't update group contents                  | Data consistency | ✅ Shipped (pending user-confirmation)                                                              |
| B8: clone-move of a group rewrites `.targets` to single wire     | Data consistency | ✅ Shipped (pending user-confirmation)                                                              |
| B9: quantum controls on a group are never drawn _(M2)_           | Display bug      | ✅ Shipped (pending user-confirmation)                                                              |
| B10: control drag on a group moves the whole group _(M3)_        | Interaction bug  | ✅ Shipped (pending user-confirmation)                                                              |
| B11: expanded-group control drag blocked + ViewState lost _(M4)_ | Interaction bug  | ✅ Shipped (pending user-confirmation)                                                              |

---

## Test coverage audit — PR readiness

A snapshot of where coverage stands at the close of the
re-architecture campaign, written up so the gap list survives across
sessions and the "what to write before the PR" decisions are
explicit.

**Current totals.** 412 tests across 22 `.mjs` files in
[test/circuit-editor/](../../test/circuit-editor/) — all passing —
plus 21 fixtures in [test/circuits-cases/](../../test/circuits-cases/)
(9 `.qsc` + 12 `.qs`) driven by the snapshot harness in
[test/circuits.js](../../test/circuits.js) (regenerate with
`--test-update-snapshots`). The three quantum-control-on-group
fixtures (`quantum-control-group.qsc`,
`quantum-control-group-collapsed.qsc`,
`quantum-control-classical-group.qsc`) were regenerated after the
group-quantum-control rendering was stripped per the design
lock-in — the renderer now omits the control dot + connector on
groups; dashed-box, child gate, and classical-control geometry
are byte-identical pre/post.

### Per-module coverage

Mapping each production module under
[ux/circuit-vis/](.) to its test file (or absence). Line counts as of
this writing.

**Data layer** — well-covered. Pure data, no JSDOM needed; the
contracts are pinned by direct unit tests against fresh model
instances.

| Module                                           | Lines | Tests                                                                         | Notes                                                                   |
| ------------------------------------------------ | ----- | ----------------------------------------------------------------------------- | ----------------------------------------------------------------------- |
| [data/circuitModel.ts](data/circuitModel.ts)     | 141   | [circuitModel.test.mjs](../../test/circuit-editor/circuitModel.test.mjs) (17) | Invariants exercised.                                                   |
| [data/location.ts](data/location.ts)             | 249   | [location.test.mjs](../../test/circuit-editor/location.test.mjs) (14)         | Parse / compose / equals / immutability locked down.                    |
| [data/viewState.ts](data/viewState.ts)           | 161   | [viewState.test.mjs](../../test/circuit-editor/viewState.test.mjs) (18)       | `expandToReveal`, default-expansion overlay, stale-key behavior pinned. |
| [data/circuit.ts](data/circuit.ts) / register.ts | 19/8  | n/a                                                                           | Pure type / structural definitions; no behavior to test.                |

**Action layer** — heavily covered on `circuitActions.ts`;
`interactionActions.ts` and `operationPrompts.ts` covered directly.

| Module                                                         | Lines | Tests                                                                                     | Notes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| -------------------------------------------------------------- | ----- | ----------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [actions/circuitActions.ts](actions/circuitActions.ts)         | 2551  | [circuitActions.test.mjs](../../test/circuit-editor/circuitActions.test.mjs) (126)        | The crown jewel: every move / add / remove / control path, plus extend cascade, classical-ref remap, clone-move, M5/B5 gates.                                                                                                                                                                                                                                                                                                                                                                                                       |
| [actions/interactionState.ts](actions/interactionState.ts)     | 97    | [interactionActions.test.mjs](../../test/circuit-editor/interactionActions.test.mjs) (10) | Defaults + reset semantics pinned.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| [actions/interactionActions.ts](actions/interactionActions.ts) | 118   | (same file)                                                                               | `beginToolboxDrag`, dropzone tracking, idempotency covered.                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| [editor/operationPrompts.ts](editor/operationPrompts.ts)       | 203   | [operationPrompts.test.mjs](../../test/circuit-editor/operationPrompts.test.mjs) (12)     | B2/B3 confirm prompts + cascade orchestration. `_deleteOperationWithConfirmation`: non-M / M-no-consumers fast paths, singular + plural prompt text, OK cascade, Cancel = no mutation + no `renderFn`. `_moveOperationWithConfirmation`: non-M / M-no-consumers fast paths, pure-survivors / pure-invalidated / mixed message shapes from `_buildMoveMConsumerMessage`, OK cascade through `moveMeasurementWithDependents`, Cancel = no mutation + no `renderFn`. `movingControl` threaded through the fast path (B11a regression). |

**View layer (controllers + editor)** — mixed. The split surfaced
by R5 made per-controller testing trivial, but only some
controllers actually got tests.

| Module                                                                                                                    | Lines           | Tests                                                                                                                                                     | Notes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| ------------------------------------------------------------------------------------------------------------------------- | --------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [editor/controllers/dragController.ts](editor/controllers/dragController.ts)                                              | 929             | [dragController.test.mjs](../../test/circuit-editor/dragController.test.mjs) (28)                                                                         | Toolbox-drag, drop commit, B11 carve-out, drag-out-delete, `commitAddControl` no-duplicate (2), `hideInvalidDropzones` / `showAllDropzones` cycle (5), D4 Stage B shift-extend lifecycle — setup / spawn / B6 block / tag-and-respawn / paint+clear ghost / tearDown (8), Ctrl-clone via `addOperation`, document-mouseup `!dragging` no-op, qubit-drag-off delegation, movingControl drag-out via `removeControl`, document-mousedown wire-dropzone cleanup.                                                              |
| [editor/controllers/selectionController.ts](editor/controllers/selectionController.ts)                                    | 111             | [selectionController.test.mjs](../../test/circuit-editor/selectionController.test.mjs) (13)                                                               | D3 closest-wire pick + `movingControl` flag well-covered.                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| [editor/controllers/qubitController.ts](editor/controllers/qubitController.ts)                                            | 135             | [qubitController.test.mjs](../../test/circuit-editor/qubitController.test.mjs) (9)                                                                        | Basic mousedown / mouseup wiring + remove-with-confirmation orchestration: prompt singular / plural message, OK click cascades through `findAndRemoveOperations` + `removeQubit` + render, Cancel leaves model + DOM untouched.                                                                                                                                                                                                                                                                                            |
| [editor/controllers/keyboardController.ts](editor/controllers/keyboardController.ts)                                      | 49              | [keyboardController.test.mjs](../../test/circuit-editor/keyboardController.test.mjs) (6)                                                                  | Complete coverage (Ctrl-toggle states + dispose).                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| [editor/controllers/scrollController.ts](editor/controllers/scrollController.ts)                                          | 77              | [scrollController.test.mjs](../../test/circuit-editor/scrollController.test.mjs) (8)                                                                      | Auto-scroll edges covered.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| [editor/contextMenu.ts](editor/contextMenu.ts)                                                                            | 345             | [contextMenu.test.mjs](../../test/circuit-editor/contextMenu.test.mjs) (13)                                                                               | Every M5 / M7 / B5 UI gate pinned via a JSDOM stub-`CircuitEvents`: measurement → only Delete; ket → only Delete; control-dot on simple parent → only Remove control; control-dot on multi-target / group parent → no menu items (B5); X-gate ordering with / without controls; multi-target unitary (M5) → no Add/Remove Control; group (M7) → no Toggle Adjoint; ordinary unitary with params / controls (full menu); re-open replaces prior menu; outside-click closes; Add Control delegates to `_startAddingControl`. |
| [editor/draggable.ts](editor/draggable.ts)                                                                                | 800             | [draggable.test.mjs](../../test/circuit-editor/draggable.test.mjs) (14) + dropzones (15)                                                                  | Pure-helper geometry pinned: `makeDropzoneBox` inter-column / on-column / trailing-append / attr contract / pathPrefix nesting, `makeShiftExtendGhost` above-span / below-span / trailing-column horizontal extend / inside-span, `createWireDropzone` on-wire / between / after-last, `removeAllWireDropzones` selective wipe. `_populateDropzonesForGrid` recursion still indirect via `dropzones.test.mjs`.                                                                                                             |
| [editor/events.ts](editor/events.ts)                                                                                      | 196             | **0 direct tests**                                                                                                                                        | Coordinator. Wiring exercised end-to-end through controllers; the controller-instantiation order and `dispose()` chain are not pinned directly.                                                                                                                                                                                                                                                                                                                                                                            |
| [editor/operationPrompts.ts](editor/operationPrompts.ts)                                                                  | 203             | [operationPrompts.test.mjs](../../test/circuit-editor/operationPrompts.test.mjs) (12)                                                                     | See action-layer table above.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| [editor/toolbox.ts](editor/toolbox.ts)                                                                                    | 169             | [toolboxRunButton.test.mjs](../../test/circuit-editor/toolboxRunButton.test.mjs) (3) + [toolbox.test.mjs](../../test/circuit-editor/toolbox.test.mjs) (5) | Run-button visibility + panel structure (header + SVG), 12 toolbox items pinned to `toolboxGateDictionary` key set, `data-type` attributes match dictionary keys, two-column grid layout (gateHeight + verticalGap pitch verified on RX/RY/Y rects — X renders as oplus so isn't used for the rect comparison), SVG height with vs without Run button differs by exactly `gateHeight + 16`. Drag-start handler still untested.                                                                                             |
| [editor/prompts.ts](editor/prompts.ts)                                                                                    | 70              | [prompts.test.mjs](../../test/circuit-editor/prompts.test.mjs) (7)                                                                                        | `_createConfirmPrompt` DOM shape (`.prompt-overlay > .prompt-container > .prompt-message + .prompt-buttons`), OK click → `callback(true)` + overlay removed, Cancel → `callback(false)` + overlay removed, Enter / Escape commit / cancel through the document-level capture-phase keydown listener, listener uninstall on close (post-close keypresses do NOT re-fire), non-Enter/Escape keys ignored.                                                                                                                    |
| [editor/shell.ts](editor/shell.ts) / standaloneRenderData.ts / installEditor.ts / toolboxGates.ts / interactionContext.ts | 100/93/62/55/55 | **0 direct tests**                                                                                                                                        | Glue / scaffolding; nothing behaviorally interesting to assert.                                                                                                                                                                                                                                                                                                                                                                                                                                                            |

**Renderer + top-level** — snapshot-only coverage on the
formatters; `sqore.ts` has no direct unit tests for its
view-state-rebase consumer logic.

| Module                                                                       | Lines | Tests                                                                                                                                             | Notes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| ---------------------------------------------------------------------------- | ----- | ------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [sqore.ts](sqore.ts)                                                         | 859   | [sqore.test.mjs](../../test/circuit-editor/sqore.test.mjs) (11) + indirect via dropzones / snapshots                                              | `rebaseViewState` consumer side pinned: first-render no-op, identity-preserved rekey, identity-lost-with-`sqore-prev-location`-stamp rekey + stamp consumption (the B11 fix), identity-lost-without-stamp drop, untracked-entry passthrough, nested-op rekey. `updateCircuit` directly pinned: swap-and-rerender preserves `viewState`, nullifies `lastLocationMap`, throws on `null` / empty / `null`-circuits inputs (matches `/No circuit found/`), and a multi-circuit group renders the first circuit.                                                                                                                                                           |
| [renderer/process.ts](renderer/process.ts)                                   | 760   | snapshot-only                                                                                                                                     | LayoutMap output partially covered by dropzone pixel tests; row-height / wire-y derivation untested directly.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| [renderer/formatters/gateFormatter.ts](renderer/formatters/gateFormatter.ts) | 867   | [gateFormatter.test.mjs](../../test/circuit-editor/gateFormatter.test.mjs) (18) + snapshots                                                       | `_getQuantumControlYs` mixed-controls filter, `_zoomButton` chevron decision tree + classical-control x-offset, `_gateBoundingBox` classical-wire inclusion + group padding, `_classicalControls` emission + B1 null-id fallback, `_createGate` CSS-class contract. SVG primitives still snapshot-only.                                                                                                                                                                                                                                                                                                                                                               |
| [renderer/formatters/\*](renderer/formatters/)                               | ~700  | snapshot-only                                                                                                                                     | inputFormatter / formatUtils / registerFormatter.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| [renderer/layoutMap.ts](renderer/layoutMap.ts)                               | 76    | indirect via dropzone pixel tests                                                                                                                 | The LayoutMap contract is tested as a side-effect of dropzone geometry assertions.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| [renderer/gateRenderData.ts](renderer/gateRenderData.ts) / constants.ts      | 97/46 | n/a                                                                                                                                               | Types / constants.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| [utils.ts](utils.ts)                                                         | 732   | [utils.test.mjs](../../test/circuit-editor/utils.test.mjs) (32) + [findOperation.test.mjs](../../test/circuit-editor/findOperation.test.mjs) (15) | Solid: `pickClosestWireIndex`, `parseWireYs`, `getOuterColumnSiblingWires`, `getAncestorColumnSiblingWires`, `getChildTargets`, find helpers.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| [angleExpression.ts](angleExpression.ts)                                     | 123   | [angleExpression.test.mjs](../../test/circuit-editor/angleExpression.test.mjs) (18) + indirect via state-viz suite                                | `isValidAngleExpression` pinned end-to-end: plain numbers, signed π in all four case forms (π / pi / Pi / PI), arithmetic combos, parenthesized + nested expressions, leading/trailing whitespace; empty / whitespace-only, unknown characters, malformed numbers (leading dot, multiple decimals), unbalanced parens, dangling operators, division-by-zero (`!isFinite` guard), and adjacent factors (no implicit multiply). `normalizeAngleExpression` pinned: whitespace trim, case-insensitive `pi` → `π` fold (including embedded), idempotency. `evaluateAngleExpression` still covered by [stateCompute.test.mjs](../../test/state-viz/stateCompute.test.mjs). |
| [index.ts](index.ts)                                                         | 53    | n/a                                                                                                                                               | Re-exports.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |

### Gap list grouped by milestone

These are the gaps where shipped feature work isn't directly
pinned by a test, organized so a reviewer can ask "is M5
regression-tested?" and find the answer.

- **M5 (refuse add/remove control on multi-target / groups).**
  Action layer ✅ (5 tests in `circuitActions.test.mjs`).
  Context-menu UI gating ✅ — multi-target unitary case in
  [contextMenu.test.mjs](../../test/circuit-editor/contextMenu.test.mjs)
  asserts `[Toggle Adjoint, Delete]` (no Add/Remove Control); the
  group case asserts `[Delete]` (groups satisfy the same predicate).
  Control-dot variant ✅ (B5 entry below).
- **M7 (hide Toggle Adjoint on groups).** Inline check in
  [contextMenu.ts](editor/contextMenu.ts) ✅ — covered by the
  group case in
  [contextMenu.test.mjs](../../test/circuit-editor/contextMenu.test.mjs)
  (`children != null` → no Toggle Adjoint).
- **B5 (no Remove control on control-dot of a multi-target /
  group parent).** ✅ — covered by the control-dot-on-multi-target
  case in
  [contextMenu.test.mjs](../../test/circuit-editor/contextMenu.test.mjs)
  (the rendered menu is empty).
- **B11 (ViewState transfer across moves).** Action layer ✅
  (3 tests pinning the `sqore-prev-location` stamp). Consumer
  side in [`Sqore.rebaseViewState`](sqore.ts) ✅ — 6 direct
  tests in [sqore.test.mjs](../../test/circuit-editor/sqore.test.mjs)
  pin the three branches (identity preserved, identity lost +
  stamp, identity lost + no stamp) plus the first-render no-op,
  untracked-entry passthrough, and nested-op rekey. Stamp
  consumption (deletion from `dataAttributes` after the rebase)
  is also asserted.
- **B2 / B3 (M-with-dependents flows).** Action layer ✅
  (10 tests in `circuitActions.test.mjs`). Confirm-prompt wrappers
  in [operationPrompts.ts](editor/operationPrompts.ts) ✅ —
  12 tests in
  [operationPrompts.test.mjs](../../test/circuit-editor/operationPrompts.test.mjs)
  pin the singular / plural delete prompts, the three move-message
  shapes (`_buildMoveMConsumerMessage`: pure-survivors,
  pure-invalidated, mixed), the OK-cascade contract for both
  wrappers, and the Cancel-path invariant (model untouched,
  `renderFn` not called). `_createConfirmPrompt` itself covered
  by [prompts.test.mjs](../../test/circuit-editor/prompts.test.mjs)
  (7 tests including Enter / Escape keyboard semantics and
  listener cleanup).
- **M2 / B9 (group-control rendering).**
  Classical-controls-on-groups path covered directly in
  `gateFormatter.test.mjs` — `_getQuantumControlYs` filter,
  `_classicalControls` emission + B1 null-id fallback,
  `_gateBoundingBox` classical-wire inclusion, `_createGate`
  CSS-class contract. The pure-quantum-controls-on-groups
  rendering originally added by M2 was permanently reverted as
  part of the team's design lock-in: groups never carry
  editor-authored quantum controls, and the renderer doesn't
  carry special-case logic for the loaded-data variant either.
  The three quantum-control-group snapshots reflect that
  (regenerated this pass).
- **D-series cascades.** All extend / overlap / split paths
  covered in `circuitActions.test.mjs`. Dropzone visibility filter
  paths covered in `dropzones.test.mjs`.

### Recommended quick-wins (cut line for the PR)

Ordered by value × cost. Items 1–5 are "Cheap" (each ~1 day or
less, no new harness investment) and are the suggested cut line
for what to land before opening the PR.

1. **`Sqore.rebaseViewState` direct unit tests.** ✅ shipped —
   6 tests in [sqore.test.mjs](../../test/circuit-editor/sqore.test.mjs)
   cover the three core branches (identity preserved, identity
   lost + `sqore-prev-location` stamp, identity lost + no stamp)
   plus the first-render no-op, untracked-entry passthrough,
   and nested-op rekey. Stamp consumption is asserted on the
   stamp branch so the marker can't leak into the rendered SVG.
2. **`_deleteOperationWithConfirmation` cancel-path test.** ✅
   shipped — covered in
   [operationPrompts.test.mjs](../../test/circuit-editor/operationPrompts.test.mjs)
   alongside the singular / plural prompt text and the OK-cascade
   path. The cancel-path test clicks the `.prompt-button` Cancel
   button (the real `_createConfirmPrompt` is exercised end-to-end
   under JSDOM rather than stubbing `window.confirm`) and asserts
   the model is byte-for-byte unchanged and `renderFn` is not
   called.
3. **`_moveOperationWithConfirmation` cascade-count assertions.**
   ✅ shipped — three message-shape tests in
   [operationPrompts.test.mjs](../../test/circuit-editor/operationPrompts.test.mjs)
   pin pure-survivors / pure-invalidated / mixed, plus a Cancel
   path and a mixed-partition OK-cascade test confirming the
   survivor remains and the invalidated consumer is gone.
4. **`isValidAngleExpression` direct tests.** ✅ shipped — 18
   tests in
   [angleExpression.test.mjs](../../test/circuit-editor/angleExpression.test.mjs)
   cover the validity contract used by the Edit Argument prompt:
   plain numbers, π / pi / Pi / PI, arithmetic + parens, whitespace
   tolerance, plus the rejection cases (empty input, unknown chars,
   malformed numbers, unbalanced parens, dangling operators,
   division-by-zero, adjacent factors). Same file pins
   `normalizeAngleExpression` (the prompt's preprocessing step).
5. **`dragController` horizontal-drag commit-path test.** ✅
   shipped — the dragController-coverage wave landed 18 new
   tests (10 pre-existing → 28), covering toolbox drop, drag-out
   delete, B11 carve-out, the full `hideInvalidDropzones` /
   `showAllDropzones` cycle, the D4 Stage B shift-extend
   lifecycle, Ctrl-clone via `addOperation`, document-mouseup
   `!dragging` no-op, qubit-drag-off delegation, movingControl
   drag-out via `removeControl`, and wire-dropzone cleanup.
   Same wave landed 14 pure-helper unit tests for
   `draggable.ts`. Outstanding sub-items: `onArgButtonClick`
   (depends on `promptForArguments` — best landed alongside the
   context-menu DOM harness below).

Larger follow-ups (deferred — not blocking PR):

- **Context-menu DOM-test harness.** ✅ shipped —
  [contextMenu.test.mjs](../../test/circuit-editor/contextMenu.test.mjs)
  (13 tests) covers M5 (multi-target unitary), M7 (group), B5
  (control-dot on multi-target / group), X-gate ordering, the
  general unitary menu with params + controls, the menu-replace
  / outside-click lifecycle, and the Add Control delegation
  contract. Edit Argument visibility is pinned (item appears
  when `params?.length > 0`); the deeper `promptForArguments`
  flow still depends on `_createInputPrompt` (the per-parameter
  text-input dialog with the π button + live validity-gated OK
  button). Validation through `isValidAngleExpression` is now
  directly covered, but the input-prompt DOM lifecycle itself
  (chained per-param prompts, π-button insertion, Escape
  cancel) remains untested.
- **`gateFormatter._renderQuantumGroupControls` geometry tests.**
  No longer applicable. The function is now unitary-only —
  groups don't call it per the design lock-in. The remaining
  single-target unitary call site is covered by
  `gateFormatter.test.mjs`'s `_getQuantumControlYs` mixed-controls
  tests plus the snapshot suite.
- **Circuit-test fixture DSL.** Nice-to-have, not blocking. The
  action-layer test files build their input circuits as nested
  `componentGrid` object literals — explicit and self-contained,
  but verbose enough that the layout under test isn't always
  obvious at a glance (a two-op, two-column group can take 30+
  lines). Two shapes worth considering if maintenance burden
  grows:
  1. **Small file-local builder helpers** — a handful of pure
     functions like `group(name, targets, cols)`, `gate(name, q, ctrls?)`,
     `M(q)`, and `circuit(nQubits, cols)` that compose into a
     one-liner per test. Lowest investment; no parsing, no
     fixture files. The structure still mirrors the data.
  2. **ASCII-diagram parser** — a test-only helper that
     accepts a textual layout (`col 0: M q2 → c2`,
     `col 1: if(c2) H q0`, etc.) and emits the
     `componentGrid` shape. Highest readability, highest
     upfront cost; only worth it if many more test files like
     `groupCollisionSplit.test.mjs` get added.

  Either form is purely additive — existing tests can keep using
  literals indefinitely. Not worth doing speculatively; revisit
  if the next round of test additions in this area starts to
  drown the assertions in setup boilerplate.

### Working principles

- **Each gap-fix test ships independently.** No long-running
  branches for test-only work; land them one or two at a time so
  reviewers can absorb them.
- **Prefer pure-data tests over JSDOM tests.** Most of the gap
  list is action-layer or pure-helper coverage; the JSDOM
  context-menu harness is the one place we'd actually need DOM
  setup, and it's deferred.
- **Don't lower the bar on existing tests.** `circuitActions.test.mjs`
  is the load-bearing test file — 126 cases — and its
  patterns should be the model for any new action-layer tests
  added during this pass.

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

#### Deferred: auto-expand on external circuit change (undo/redo)

When a host pushes a `CircuitGroup` whose change is inside a
collapsed group, the View layer should auto-expand the changed
op's ancestors so the change is immediately visible — otherwise a
user who collapses a group and then hits Ctrl+Z on something
inside it has to go hunting for the difference.

**Status: spec'd, partially implemented, not yet working
end-to-end in VS Code.** A first attempt has been stashed for
future work.

**What was built (and works at the npm-package level):**

- A pure helper `diffChangedScopes(oldGrid, newGrid)` in a new
  `data/circuitDiff.ts` that returns the set of _scope locations_
  containing changes. Per-op shallow JSON compare; recurses into
  matching children; structural mismatches (column or per-column
  component count) report the current scope and stop descending.
- `ViewState.expandToReveal(location)` walks a location string
  and marks every ancestor (and the location itself) as
  expanded, overriding any prior user collapse on the path.
- `Sqore.updateCircuit` calls
  `diffChangedScopes(oldGrid, newGrid)` and routes each scope
  through `viewState.expandToReveal` before swapping.
- 22 new tests covering all of the above (14 in
  `circuitDiff.test.mjs`, 5 in `viewState.test.mjs`, 3 in
  `dropzones.test.mjs` including the "override prior user
  collapse" case and the "logically-equal push doesn't
  auto-expand" guard). All pass.

**Where it breaks:** when the user moves an op inside a
collapsed group in VS Code and hits Ctrl+Z, the auto-expand
doesn't fire. Adds/removes sometimes do, sometimes don't. The
fix-attempt to mirror `state.props.circuit` on edit (so the
webview's dedup compares against what's actually displayed)
didn't fully resolve it. Suspect one of:

1. **Webview dedup over- or under-firing** because of property
   ordering / number normalization differences between
   in-memory objects and `JSON.parse` round-trips. Worth logging
   `state.props.circuit` and `message.props.circuit` at the
   dedup point in
   [vscode/src/webview/editor.tsx](../../../../vscode/src/webview/editor.tsx)
   to see what they actually look like on undo.
2. **Preact remount of `ZoomableCircuit`** dropping
   `qvizObj.current`, sending the path through the initial-mount
   branch (which constructs a new Sqore and discards
   `viewState`). The `editor` object is rebuilt inside `App` on
   every render in
   [vscode/src/webview/editor.tsx](../../../../vscode/src/webview/editor.tsx),
   which can change identity even when its contents don't.
3. **Text-doc echo loop** in
   [vscode/src/circuitEditor.ts](../../../../vscode/src/circuitEditor.ts)
   doing something subtler than the `updatingDocument` guard
   suggests — e.g. firing `onDidChangeTextDocument` after the
   guard has already cleared.

**Resume plan:** add `console.log` lines at three points (dedup
result in `editor.tsx`, `changedScopes` in `sqore.ts`
`updateCircuit`, branch taken in `circuit.tsx`'s
`useEffect([props.circuitGroup])`), reproduce in VS Code, and
follow whichever logs show the unexpected behavior. The
npm-package code is correct and unit-tested; the bug is in the
VS Code integration glue.

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

### 10. Comment audit across circuit-editor files

**Goal:** Trim verbose comments throughout the circuit editor and
renderer code. The current style accumulated long historical
narrative (past bugs, prior implementations, "why we changed X")
that makes files larger and harder to read/review.

**Rules:**

- Describe the code as it is, not past states or fixed bugs.
- Brief — prefer one line over a paragraph; drop a comment entirely
  when the code is self-evident.
- Keep JSDoc on public/exported symbols and on non-obvious
  invariants; cut redundant "what" narration.

**Scope:** `ux/circuit-vis/**` — actions, data, editor, renderer,
and the editor test files' inline narration.

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
