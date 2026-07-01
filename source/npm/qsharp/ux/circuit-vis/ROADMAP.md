# Circuit Editor — Roadmap (user-facing summary)

A concise status overview of the Circuit Editor (CE) workstream. For
the full detail (rationale, design decisions, code references), see
[CIRCUIT_EDITOR_TODO.md](CIRCUIT_EDITOR_TODO.md).

The roadmap has three phases:

1. **Architecture refactor** (R1–R7) — foundational rework.
2. **Drag-and-drop overhaul** (Phase A/B/C + D-series bug bash) —
   making the editor actually usable.
3. **Planned features** (#1–#9) — the authoring story users will
   see.

The first phase is fully done, the second is almost done with one
significant phase remaining, and the third is largely untouched.

---

## [Architecture refactor (R1–R7)](CIRCUIT_EDITOR_TODO.md#architecture-refactor--prerequisite-for-further-editor-work) — complete

Three-layer **Data + Actions + View** split. Done. This is the
foundation everything below builds on.

|                                                                                                 |                                                           | Status |
| ----------------------------------------------------------------------------------------------- | --------------------------------------------------------- | ------ |
| [R1](CIRCUIT_EDITOR_TODO.md#r1--layoutmap-as-a-first-class-output-of-processoperations--done)   | `LayoutMap` as first-class output of `processOperations`  | ✅     |
| [R2](CIRCUIT_EDITOR_TODO.md#r2--retry-edit-inside-groups-with-layoutmap-the-real-phase-a--done) | Editing inside groups (the real Phase A, via R1)          | ✅     |
| [R3](CIRCUIT_EDITOR_TODO.md#r3--data-layer-circuitmodel--action-layer-circuitactions--done)     | `CircuitModel` (Data) + `CircuitActions` (Actions)        | ✅     |
| [R3.5](CIRCUIT_EDITOR_TODO.md#r35--action-layer-interactionstate--interactionactions--done)     | `InteractionState` + `InteractionActions`                 | ✅     |
| [R4](CIRCUIT_EDITOR_TODO.md#r4--data-layer-location-value-type--done)                           | `Location` value type (replaces stringly-typed paths)     | ✅     |
| [R5](CIRCUIT_EDITOR_TODO.md#r5--view-layer-split-circuitevents-into-focused-controllers--done)  | Split `CircuitEvents` into focused controllers            | ✅     |
| [R6](CIRCUIT_EDITOR_TODO.md#r6--view-layer-editor-overlay--done)                                | Editor overlay (separate `<g class="editor-overlay">`)    | ✅     |
| [R7](CIRCUIT_EDITOR_TODO.md#r7--action-layer-split-the-monolithic-circuitactions-file--done)    | Split `circuitActions.ts` into `circuit-actions/` modules | ✅     |

---

## [Drag-and-drop overhaul](CIRCUIT_EDITOR_TODO.md#drag-and-drop-overhaul)

|                                                                                     |                                     | Status                               |
| ----------------------------------------------------------------------------------- | ----------------------------------- | ------------------------------------ |
| Phase A                                                                             | Lift "no editing inside groups"     | ✅ (via R1+R2)                       |
| **[Phase B](CIRCUIT_EDITOR_TODO.md#phase-b--make-multi-target-dropping-reachable)** | **Multi-target dropping reachable** | ⏳ TODO — needs Inspector (#2) first |
| Phase C                                                                             | State-machine + PointerEvents       | ✅ (superseded by R5+R6)             |

**Phase B is the only remaining phase here.** Plan: drop from
toolbox always creates a 1-target gate; if the gate's arity
requires more, auto-open the Inspector for pick-mode. Depends on
Inspector (#2) shipping first.

### D-series (post-refactor bug bash) — all addressed

Captured from the GroupSplittingTest bug bash.

|                                                                                                         |                                                                     | Status      |
| ------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------- | ----------- |
| [D1](CIRCUIT_EDITOR_TODO.md#d1-crash-when-a-group-is-emptied-by-a-move-out)                             | Crash when group emptied by move-out                                | ✅          |
| [D2](CIRCUIT_EDITOR_TODO.md#d2-move-group-containing-a-classical-condition-above-its-producer--shipped) | Conditional moved before producer M                                 | ✅          |
| [D3](CIRCUIT_EDITOR_TODO.md#d3-multi-target-gate--group-movement-semantics)                             | Multi-target movement semantics                                     | ✅          |
| [D4](CIRCUIT_EDITOR_TODO.md#d4-move-inside-group-vs-promote-out-of-group-disambiguation)                | Move-out vs. expand-group                                           | ✅          |
| [D5](CIRCUIT_EDITOR_TODO.md#d5-dropzone-overlapping-rendered-gate--shipped)                             | Dropzone overlapping rendered gate                                  | ✅          |
| [D6](CIRCUIT_EDITOR_TODO.md#d6--pure-derived-group-targets-investigated-rejected)                       | Pure-derived group `.targets` (rejected — eager cache wins on perf) | ❌ rejected |
| [D7](CIRCUIT_EDITOR_TODO.md#d7--centralized-bottom-up-ancestor-targets-refresh-utility)                 | Centralized ancestor-targets refresh                                | ✅          |

---

## [Bug fixes](CIRCUIT_EDITOR_TODO.md#bug-fixes--open)

Reproducible regressions from recent editor flows. Listed by
severity (crashes first).

|                                                                                                                                                                             |                                                                        | Status                                                                                              |
| --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| [B1](CIRCUIT_EDITOR_TODO.md#b1-classical-control-indicators-always-show-c_null--partial)                                                                                    | Classical-control indicators always show `C_null`                      | ⚠️ Partial — immediate symptom fixed; architectural fix deferred to future editor-authoring feature |
| [B2](CIRCUIT_EDITOR_TODO.md#b2-moving--deleting-an-m-that-later-gates-depend-on-crashes--shipped-pending-user-confirmation)                                                 | Moving / deleting M with downstream deps crashes                       | ✅ Shipped (pending user-confirmation)                                                              |
| [B3](CIRCUIT_EDITOR_TODO.md#b3-moving-qubits-around-an-m-that-later-gates-depend-on-crashes)                                                                                | Qubit reorder around dependent M crashes                               | ✅ Shipped (user-confirmed)                                                                         |
| [B4](CIRCUIT_EDITOR_TODO.md#b4-removing-an-m-doesnt-update-later-classical-wire-positions)                                                                                  | M removal leaves stale classical wire layout                           | ✅ Subsumed by B2's cascade-delete (pending user-confirmation)                                      |
| [B5](CIRCUIT_EDITOR_TODO.md#b5-add--remove-control-on-a-classically-controlled-op-blocked-by-classical-ref-entries--shipped-pending-user-confirmation)                      | Add/remove control fails on classical groups                           | ✅ Shipped (pending user-confirmation)                                                              |
| [B6](CIRCUIT_EDITOR_TODO.md#b6-shift-extend-onto--past-an-external-sibling--shipped-pending-user-confirmation)                                                              | Shift-extend onto / past an external sibling                           | ✅ Shipped (pending user-confirmation)                                                              |
| [B7](CIRCUIT_EDITOR_TODO.md#b7-qubit-rearrangement-doesnt-update-group-contents-correctly--shipped)                                                                         | Qubit reorder doesn't update group contents                            | ✅ Shipped (pending user-confirmation)                                                              |
| [B8](CIRCUIT_EDITOR_TODO.md#b8-clone-move-of-a-multi-wire-group-rewrites-targets-to-a-single-wire-stub--shipped)                                                            | Clone-move of a group rewrites `.targets` to single wire               | ✅ Shipped (pending user-confirmation)                                                              |
| [B9](CIRCUIT_EDITOR_TODO.md#b9-quantum-controls-on-a-group-are-never-drawn--shipped-pending-user-confirmation)                                                              | Quantum controls on a group are never drawn                            | ✅ Shipped (pending user-confirmation)                                                              |
| [B10](CIRCUIT_EDITOR_TODO.md#b10-control-drag-on-a-group-moves-the-whole-group-instead-of-just-the-control--shipped-pending-user-confirmation)                              | Control drag on a group moves the whole group                          | ✅ Shipped (pending user-confirmation)                                                              |
| [B11](CIRCUIT_EDITOR_TODO.md#b11-control-drag-on-a-group-expanded-groups-blocked--classically-controlled-groups-re-expand-on-every-move--shipped-pending-user-confirmation) | Control drag on a group: expanded blocked + classical groups re-expand | ✅ Shipped (pending user-confirmation)                                                              |

**B1 is the only bug entry not fully closed.** Its visible
symptom (`C_null` label) is fixed by a render-side fallback;
the architectural piece (global classical-register id assignment)
is deliberately deferred until the editor-authoring feature gives
a second concrete producer to anchor the design (see B1 detail).

The B2/B3/B4 trio (classical-register reference integrity under
M edits and qubit reorders) is closed: B2's `_applyClassicalRefRemap`

- cascade-delete pass, B3's structural immunity via `moveQubit`'s
  1-to-1 wire permutation (regression-tested in the
  [circuit-actions/](../../test/circuit-editor/circuit-actions/)
  suite), and B4's subsumption by B2's cascade together remove the
  entire class of orphaned-classical-ref crashes.

---

## [Test coverage audit](CIRCUIT_EDITOR_TODO.md#test-coverage-audit--pr-readiness) — PR readiness

A snapshot of where coverage stands at the close of the
re-architecture campaign, focused on what's worth landing before
the PR opens.

**Current totals.** 420 tests across 30 `.mjs` files in
[test/circuit-editor/](../../test/circuit-editor/) — all passing —
plus 21 snapshot fixtures in
[test/circuits-cases/](../../test/circuits-cases/). Action-layer
cases live in the
[circuit-actions/](../../test/circuit-editor/circuit-actions/)
subfolder (10 files); the other 20 sit at the top level.

**Strongest areas.**

- Data layer (`circuitModel`, `location`, `viewState`) — direct
  unit tests, no JSDOM.
- Action layer — the [circuit-actions/](../../test/circuit-editor/circuit-actions/)
  suite (10 files, 132 tests, split out from the former monolithic
  `circuitActions.test.mjs`) is the crown jewel; pins every move /
  control / extend-cascade / classical-ref-remap / clone-move path
  along with the M5 / B5 / B6 gates.
- Utilities — [`utils.test.mjs`](../../test/circuit-editor/utils.test.mjs)
  (32 tests) covers wire-pick, parse-wire-Ys, sibling-wire
  enumeration, child-target derivation, and find helpers.

**Highest-value gaps** (full per-module table in the TODO):

| Surface                                             | Status                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| --------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Sqore.rebaseViewState` (B11 consumer side)         | ✅ 6 direct tests in [sqore.test.mjs](../../test/circuit-editor/sqore.test.mjs) pin the three branches (identity preserved, identity lost + stamp, identity lost + no stamp) plus the first-render no-op, untracked-entry passthrough, and nested-op rekey. Stamp consumption asserted.                                                                                                                                                                                                                                |
| `prompts.ts` (primitive + B2 / B3 flows)            | ✅ 19 direct tests in [prompts.test.mjs](../../test/circuit-editor/prompts.test.mjs): 7 pin the `createConfirmPrompt` primitive (DOM shape, OK / Cancel click semantics, Enter / Escape keyboard semantics, listener cleanup, ignore-other-keys); 12 cover the delete/move flows end-to-end under JSDOM (fast paths, singular / plural delete prompts, the three move-message shapes — pure-survivors / pure-invalidated / mixed, OK-cascade contracts, and the Cancel-path invariant of no mutation + no `renderFn`). |
| `contextMenu.ts` (M5 / M7 / B5 UI gates)            | ✅ 13 direct tests in [contextMenu.test.mjs](../../test/circuit-editor/contextMenu.test.mjs) cover every kind branch (measurement, ket), control-dot on simple / multi-target parent (B5), X-gate ordering with / without controls, M5 (multi-target unitary), M7 (group), ordinary unitary with / without controls + params, menu-replace + outside-click lifecycle, and `_startAddingControl` delegation.                                                                                                            |
| `dragController.ts` (929 lines, 26 tests)           | ✅ Major paths covered: toolbox drop, drag-out delete, B11 carve-out, `commitAddControl` no-duplicate, `hideInvalidDropzones` / `showAllDropzones` cycle, D4 Stage B shift-extend lifecycle, Ctrl-clone, document-mouseup `!dragging` no-op, qubit-drag-off, movingControl drag-out, wire-dropzone cleanup. `onArgButtonClick` waits on the deferred context-menu DOM harness.                                                                                                                                         |
| `draggable.ts` (800 lines, 14 direct + 15 dropzone) | ✅ Pure-helper geometry pinned: `makeDropzoneBox` / `makeShiftExtendGhost` / `createWireDropzone` / `removeAllWireDropzones`. `_populateDropzonesForGrid` recursion still indirect via `dropzones.test.mjs`; no dead code found in the audit.                                                                                                                                                                                                                                                                          |
| `gateFormatter.ts` group-control geometry (M2/B9)   | ⚠️ Classical-controls-on-groups path covered directly (`_getQuantumControlYs`, `_classicalControls`, `_gateBoundingBox`, `_createGate` — 18 tests in `gateFormatter.test.mjs`). Quantum-controls-on-groups geometry still snapshot-only; deferred with M6.                                                                                                                                                                                                                                                             |
| `isValidAngleExpression` (Edit Argument flow)       | ✅ 18 tests in [angleExpression.test.mjs](../../test/circuit-editor/angleExpression.test.mjs) pin the validity contract used by the Edit Argument prompt (numbers, π in all four case forms, arithmetic + parens, whitespace tolerance, plus the full rejection set) and the `normalizeAngleExpression` preprocessing step (trim + case-insensitive `pi` → `π` fold + idempotency). `evaluateAngleExpression` itself remains covered by the state-viz suite.                                                           |

**Cut line for the PR** (~5 cheap items, each ~1 day):

1. `Sqore.rebaseViewState` unit tests (identity preserved /
   identity lost + stamp present / stamp absent).
   ✅ shipped — 6 tests in
   [sqore.test.mjs](../../test/circuit-editor/sqore.test.mjs)
   covering all three branches plus first-render no-op,
   untracked-entry passthrough, and nested-op rekey.
2. `deleteOperationWithConfirmation` cancel-path test.
   ✅ shipped — covered in
   [prompts.test.mjs](../../test/circuit-editor/prompts.test.mjs)
   alongside singular / plural prompt text and the OK-cascade
   path (12 flow tests total).
3. `moveOperationWithConfirmation` cascade-count message tests
   (pure-survivors, pure-invalidated, mixed).
   ✅ shipped — three message-shape tests in the same file pin
   each branch of `_buildMoveMConsumerMessage`, plus a Cancel
   path and a mixed-partition OK-cascade test.
4. `isValidAngleExpression` direct tests.
   ✅ shipped — 18 tests in
   [angleExpression.test.mjs](../../test/circuit-editor/angleExpression.test.mjs)
   cover the validity contract end-to-end and also pin
   `normalizeAngleExpression` (the prompt's preprocessing step).
5. `dragController` horizontal control-drag commit-path test.
   ✅ shipped — same wave also closed out the `draggable.ts`
   audit (14 pure-helper tests; no dead code found).

Architectural follow-ups surfaced during the pre-PR test sweep
(must resolve before the PR, but not blocking the rest of the
test reorg first):

- **Wire-range helper consolidation.** `utils.ts` has three
  close-but-not-identical helpers (`getMinMaxRegIdx`,
  `getQuantumWireRange`, `getWireRange`) for "what wires does
  this op touch." 🔶 partially shipped — audit done: all four
  `getQuantumWireRange` sites are intentional quantum-only
  carve-outs (nothing switches to `getWireRange`), and
  `getMinMaxRegIdx` was de-duped onto `getOperationRegisters`.
  The only remaining item — folding `getMinMaxRegIdx`'s numeric
  output out of `getWireRange` — is hot-path and low-value, left
  deferred. See
  [Wire-range helper consolidation — partially shipped](CIRCUIT_EDITOR_TODO.md#wire-range-helper-consolidation--partially-shipped).
- **`findAndRemoveOperations` should be action-layer internal.**
  ✅ shipped — new public `removeQubitWithDependents` now owns the
  strip-then-`removeQubit` cascade (matching the `*WithDependents`
  measurement pattern), and the primitive is now fully module-private
  `_findAndRemoveOperations` (no longer exported). `qubitController`
  collapses to a single call and no longer needs the
  `getOperationRegisters` / `Operation` imports; all tests go through
  the public action (no test imports the primitive). See
  [`findAndRemoveOperations` should be action-layer internal — deferred](CIRCUIT_EDITOR_TODO.md#findandremoveoperations-should-be-action-layer-internal--deferred).
- **`makeDropzoneBox` API is too heavy.** Fuses geometry
  (column/wire/mode → SVG rect) with identity (`data-dropzone-*`
  attrs). ✅ shipped — the testability half (pure `dropzoneRect`
  - exported `columnGeometry`) plus the signature half: the
    7 positional args are now two intent-named structs,
    `makeDropzoneBox(ctx: DropzoneContext, target: DropzoneTarget)`,
    split by what's loop-invariant (scope/wireData/pathPrefix) vs
    what varies per dropzone (colIndex/opIndex/wireIndex/interColumn).
    The earlier "identity object" idea was dropped — the
    geometry/identity seam isn't clean (`colIndex` is also a
    geometry input). See
    [`makeDropzoneBox` API is too heavy — ✅ shipped](CIRCUIT_EDITOR_TODO.md#makedropzonebox-api-is-too-heavy--%E2%9C%85-shipped).

Deferred follow-ups (not blocking PR):

- **`_`-prefix naming convention audit** — the `_` prefix is used
  inconsistently across `circuit-vis`: some `_` symbols are truly
  file-private, some are exported only for tests, and some (e.g.
  `_isMultiTargetOrGroup`) are genuine cross-module API yet still
  underscored. Proposed rule: `_` ⇔ test-only / file-private; any
  symbol a sibling module imports as normal API drops the `_`. A
  cross-cutting rename, deferred to a dedicated pass. See
  [`_`-prefix naming convention audit — deferred](CIRCUIT_EDITOR_TODO.md#_-prefix-naming-convention-audit--deferred).
- **Context-menu DOM-test harness** — ✅ shipped.
  [contextMenu.test.mjs](../../test/circuit-editor/contextMenu.test.mjs)
  (13 tests) covers M5 / M7 / B5 plus Edit Argument visibility,
  X-gate ordering, the general unitary menu, and the
  menu-replace / outside-click lifecycle. The deeper
  `promptForArguments` flow still depends on the
  `_createInputPrompt` DOM lifecycle (chained per-param
  prompts, π-button insertion, Escape cancel); validation
  through `isValidAngleExpression` is now directly covered.
- **Renderer geometry tests** for
  `_renderQuantumGroupControls` — bundled with the deferred M6
  work, since the rendering rule is expected to change there.
- **Circuit-test fixture DSL** — ✅ shipped. Built as a shared
  [\_helpers.mjs](../../test/circuit-editor/_helpers.mjs) module of
  builder helpers (`gate`, `meas`, `group`, `circuit`, `build`,
  `qubits`) plus extraction/assertion helpers (`at`, `expectGrid`
  / `expectOp`, and the `assert*Shape` / `assert*Wires` family).
  Applied across the action-layer suites in
  [circuit-actions/](../../test/circuit-editor/circuit-actions/)
  and the controller + `sqore` test files, collapsing the former
  multi-level `componentGrid` literals to one-liners. Purely
  additive — remaining hand-rolled literals can migrate
  opportunistically. The ASCII-diagram-parser alternative was
  not pursued (deferred until many more collision-split-style
  files would justify the upfront cost).

See [the full audit in the TODO](CIRCUIT_EDITOR_TODO.md#test-coverage-audit--pr-readiness)
for the per-module table, milestone-grouped gap list, and
working principles.

---

## [Planned features (in priority order)](CIRCUIT_EDITOR_TODO.md#planned-in-priority-order)

|                                                                                                                     | Feature                                                              | Status                                                                                                                                                                                                 |
| ------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **[1](CIRCUIT_EDITOR_TODO.md#1-persistent-view-state-across-re-renders--in-memory-done-host-persistence-deferred)** | **Persistent view state**                                            | ⚠️ In-memory ✅; external-update preservation ✅; host persistence (webview reload / VS Code restart) deferred; auto-expand on external change spec'd + partially built but not wired into VS Code yet |
| **[2](CIRCUIT_EDITOR_TODO.md#2-gate-inspector-panel--multi-target-editing)**                                        | **Gate Inspector panel — multi-target editing**                      | 📋 Planned. **Blocks Phase B.** Replaces today's ad-hoc context menu + single-input prompts                                                                                                            |
| **[3](CIRCUIT_EDITOR_TODO.md#3-snapshot-tool--extract-selection-into-a-custom-gate)**                               | **Snapshot tool — extract selection into custom gate**               | 📋 Planned. Needs selection model + extraction transform + persistence choice (in-doc vs. separate `.qsc`)                                                                                             |
| **[4](CIRCUIT_EDITOR_TODO.md#4-custom-gate-palette-in-the-toolbox)**                                                | **Custom-gate palette in toolbox**                                   | 📋 Planned. Depends on #3                                                                                                                                                                              |
| **[5](CIRCUIT_EDITOR_TODO.md#5-structural-group-authoring-for--if)**                                                | **Structural-group authoring (`for` / `if`)**                        | 📋 Planned. Graduate the `// loop:` / `// if:` comment fallbacks in the Rust emitter to real `for` / `if` blocks                                                                                       |
| **[6](CIRCUIT_EDITOR_TODO.md#6-controlled-adjoint-extracted-gate-test-coverage)**                                   | **Controlled-Adjoint extracted-gate test coverage**                  | 📋 Small Rust-side test gap                                                                                                                                                                            |
| **[7](CIRCUIT_EDITOR_TODO.md#7-vs-code-integration-tests-for-the-preview-pipeline)**                                | **VS Code integration tests for preview pipeline**                   | 📋 Today's coverage is heavy Rust-side, light VS Code-side                                                                                                                                             |
| **[8](CIRCUIT_EDITOR_TODO.md#8-round-trip-validation-qs--qsc--preview-q-matches-qs)**                               | **Round-trip validation: `.qs` → `.qsc` → preview Q# matches `.qs`** | 📋 Each direction tested independently today; full loop missing                                                                                                                                        |
| **[9](CIRCUIT_EDITOR_TODO.md#9-changelog--release-notes)**                                                          | **CHANGELOG / release notes**                                        | 📋 Surface the editor-parity work to users                                                                                                                                                             |
| **[10](CIRCUIT_EDITOR_TODO.md#10-comment-audit-across-circuit-editor-files)**                                       | **Comment audit across circuit-editor files**                        | 📋 Trim historical / narrative comments throughout `ux/circuit-vis/**`; describe code as-is, not past states                                                                                           |

---

## WIP on other branches — ships after rearchitecting

The following work is built but **not yet shipped**; it lives on
side branches waiting for the architecture refactor + drag-and-drop
overhaul to land first. Ship order:

1. Architecture refactor (done).
2. Drag-and-drop overhaul, including Phase B (in progress).
3. Then merge / land the items below.

- Recursive Q# emission for nested structural groups (`loop:`,
  `if:`, `<scope>`, iteration markers).
- Live Q# preview pipeline (`qsharp-circuit-preview` URI scheme,
  lazy regeneration on first load).
- Trace-divergence banner (divergent loop iterations, opaque
  conditionals).
- "Save as Circuit (.qsc)" bridge from the Show-Circuit panel.
- Custom-gate extraction with transitive closure and
  measurement-aware return types.
- Top-level entry-point wrapper unwrap (one-shot, never recursive).
- Filename-to-identifier sanitization (Rust + TypeScript mirror).
- Custom-gate call-site array-wrap convention (`Foo([qs[0], qs[1]])`
  matching `(qs : Qubit[])` signatures).

---

## Critical path (suggested next move)

1. **Inspector panel (#2)** is the biggest unblocking item — it
   gates **Phase B** (multi-target dropping) and is the natural
   surface for #3 (extraction) and #4 (custom-gate palette).
2. **Selection model (part of #3)** is a co-prerequisite — the
   Inspector edits one gate, but extraction needs region selection.
3. After Inspector + selection land, the chain
   **#2 → Phase B → #3 → #4** unlocks the rest of the authoring
   story.
4. **#5** (structural-group authoring) and **#8** (round-trip
   validation) are largely independent and can run in parallel
   with the authoring chain.
5. **#6, #7, #9** are small / cleanup items.

---

## Known limitations carried forward

- **[`ViewState` keyed by location string](CIRCUIT_EDITOR_TODO.md#1-persistent-view-state-across-re-renders---in-memory-done-host-persistence-deferred)** — edits that shift an
  op's position stale-out the entry. Stable IDs (R4 set up the
  centralization needed) are the long-term fix.
- **[Auto-expand on external change](CIRCUIT_EDITOR_TODO.md#deferred-auto-expand-on-external-circuit-change-undoredo)** — works at the npm-package
  level (`diffChangedScopes`, `expandToReveal`), but the VS Code
  wiring isn't landed yet. Stashed for future work.
- **[Host persistence (webview reload / VS Code restart)](CIRCUIT_EDITOR_TODO.md#deferred-host-persistence-webview-reload--vs-code-restart)** —
  `ViewState` resets when the circuit tab is closed and reopened
  or the window reloads. Deferred; visible pain is minor.
- **[Quantum-control authoring on multi-target ops / groups](CIRCUIT_EDITOR_TODO.md#m6-deferred-quantum-control-rendering-on-multi-target-bodies)** —
  The editor refuses `addControl` / `removeControl` on any op
  with `children` or more than one target (groups, SWAP-shaped
  unitaries, multi-qubit measurements). Pre-existing controls on
  such ops loaded from `.qsc` files still render and can still
  be moved (control-leg drag is permutation-only) — they just
  can't be created or destroyed through the editor surface.
  Lifted by the deferred **M6** milestone, which ships a
  unified rendering rule (Option C: per-control connector to
  nearest sub-box edge) for quantum controls on multi-target
  bodies.
- **[Adjoint authoring on groups](CIRCUIT_EDITOR_TODO.md#m8-deferred-adjoint-authoring-on-groups)** —
  The context-menu "Toggle Adjoint" entry is hidden on every
  group (any op with `children`). Leaf unitaries still get it.
  Pre-existing adjointed groups in loaded `.qsc` files still
  parse and render (the dagger overlay paints as before); the
  user just can't toggle the flag from the editor surface.
  Lifted by the deferred **M8** milestone, which ships an
  adjointability predicate plus emitter support for propagating
  the group-level `isAdjoint` flag into the child sequence.

---

## Open questions (not gating)

- Inspector structural-group authoring: one surface or split
  loop/conditional?
- Custom-gate palette: eager workspace scan for `.qsc` vs. lazy
  on toolbox-open?
- Separate `.qsc` references: filename only, or content hash too?
