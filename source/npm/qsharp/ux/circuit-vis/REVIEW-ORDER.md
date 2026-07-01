# Circuit-vis production code — recommended review order

A reading plan for reviewing the re-architected `circuit-vis` source.
Companion to [ARCHITECTURE.md](./ARCHITECTURE.md) (the structural
walkthrough) — this file is the **order** to read things in, and what
to look for in each batch.

## How to use this

Review **bottom-up**, following the dependency arrows. The hard rule
that drives the layering is: **`data/` and `actions/` are pure and
never touch the DOM**, the `renderer/` reads data to produce SVG, and
`editor/` glues DOM events to actions. If you read top-down you'll
keep hitting symbols you haven't met yet; bottom-up, every file only
depends on batches you've already seen.

```
data/  →  actions/  →  renderer/  →  editor/  →  sqore.ts  →  state-viz/
(pure)    (pure)       (read data)   (DOM glue)  (entrypoint) (parallel)
```

Read [ARCHITECTURE.md](./ARCHITECTURE.md) first (the TL;DR, the module
map, and the two end-to-end flow walkthroughs). It's the map; this is
the itinerary.

Line counts below are a rough effort gauge, not a quality signal. The
heaviest clusters — [circuitActions.ts](./actions/circuitActions.ts)
(1,145) plus its five [circuit-actions/](./actions/circuit-actions/)
helper modules (~1,740 combined), and
[dragController.ts](./editor/controllers/dragController.ts) (997) —
deserve the most time and have the heaviest test coverage backing them.

---

## Batch 1 — Data layer (`data/`) · ~720 lines

The vocabulary everything else is written in. Small, pure, no DOM.
Start here so every type downstream is already familiar.

| order | file                                           | lines | what to check                                                                                                                                                                                                             |
| ----- | ---------------------------------------------- | ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1     | [data/register.ts](./data/register.ts)         | 9     | trivial ID helpers — warm-up                                                                                                                                                                                              |
| 2     | [data/circuit.ts](./data/circuit.ts)           | 21    | the on-disk JSON shape (`Circuit`/`ComponentGrid`/`Operation`/`Qubit`); everything serializes to this                                                                                                                     |
| 3     | [data/location.ts](./data/location.ts)         | 264   | hierarchical address value type. Owns the `"0,1-2,3"` parse/compose in one place. Check immutability (`parent()`/`child()` return new instances) and the `root()` empty case                                              |
| 4     | [data/circuitModel.ts](./data/circuitModel.ts) | 152   | wraps a `Circuit`, maintains `qubitUseCounts`, owns invariants (`removeTrailingUnusedQubits`, `ensureQubitCount`). Confirm it borrows `componentGrid`/`qubits` **by reference**, and that it does _no_ user-level editing |
| 5     | [data/viewState.ts](./data/viewState.ts)       | 167   | per-session expand/collapse prefs, deliberately **not** serialized. The "third lifetime" distinct from model + interaction state                                                                                          |

**Focus:** Is the data/DOM boundary actually clean (no DOM imports
here)? Are the three lifetimes — persisted (`CircuitModel`), session
view prefs (`ViewState`), single-gesture (`InteractionState`, batch 2)
— kept clearly separate?

---

## Batch 2 — Action layer (`actions/`) · ~3,120 lines

Pure mutations against the data layer. The heart of the editor's
correctness. [circuitActions.ts](./actions/circuitActions.ts) is the
orchestration + public-API barrel; the mechanical helpers it composes
live in five focused modules under
[circuit-actions/](./actions/circuit-actions/). Read those bottom-up
first (they sit below `circuitActions.ts` in the import DAG), then the
orchestrator — budget accordingly.

| order | file                                                                             | lines | what to check                                                                                                                                                                                                                                                |
| ----- | -------------------------------------------------------------------------------- | ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 6     | [actions/interactionState.ts](./actions/interactionState.ts)                     | 105   | ephemeral session-state container (plain fields, no methods). Read before the controllers that mutate it                                                                                                                                                     |
| 7     | [actions/interactionActions.ts](./actions/interactionActions.ts)                 | 129   | the multi-step transitions on `InteractionState` (`resetTransient`, `beginToolboxDrag`, …). Note the one DOM-touching helper, `clearTemporaryDropzones`                                                                                                      |
| 8a    | [circuit-actions/gridPrimitives.ts](./actions/circuit-actions/gridPrimitives.ts) | 319   | leaf of the import DAG: column insert/remove (`addOp`/`removeOp`), sibling-overlap detection, drawn-span measurement, per-wire measurement renumbering. Depends only on Data + `utils.ts`                                                                    |
| 8b    | [circuit-actions/ancestors.ts](./actions/circuit-actions/ancestors.ts)           | 182   | ancestor-chain capture as `(op, containingArray)` object refs taken BEFORE any mutation (location strings don't survive mid-mutation column splices)                                                                                                         |
| 8c    | [circuit-actions/classicalRefs.ts](./actions/circuit-actions/classicalRefs.ts)   | 357   | classical-register producer/consumer analysis: the M-produces / classically-controlled-consumes graph, document-order constraints, result-index remaps                                                                                                       |
| 8d    | [circuit-actions/derivedTargets.ts](./actions/circuit-actions/derivedTargets.ts) | 479   | the eager `.targets` cache + ancestor-refresh cascade (see [circuitTargets.bench.md](../../test/circuit-editor/circuitTargets.bench.md) for _why_ the cache is eager). Depends on 8a + 8b                                                                    |
| 8e    | [circuit-actions/move.ts](./actions/circuit-actions/move.ts)                     | 402   | the geometry of a move: horizontal (`moveX`) + vertical (`moveY`) + rigid-unit register shifting. Depends on 8a + 8c + 8d                                                                                                                                    |
| 8f    | [actions/circuitActions.ts](./actions/circuitActions.ts)                         | 1,145 | **the orchestrator + public barrel.** `addOperation`/`moveOperation`/`addControl`/`removeQubit`/etc. against `CircuitModel`, composing 8a–8e. Pure data, no DOM. Pay attention to the group split/merge paths and the `*WithDependents` measurement cascades |

**Focus:** This is where the test suite you just reviewed points. For
[circuitActions.ts](./actions/circuitActions.ts), cross-reference the
[circuit-actions/](../../test/circuit-editor/circuit-actions/) test
suite — each topic file there maps to a cluster of functions here
(addRemove, groupMove, measurementCascade, producerOrdering, …). Verify
every mutator keeps `.targets` authoritative on the way out. The
2,696-line monolith was split (R7) into the orchestrator plus the five
[circuit-actions/](./actions/circuit-actions/) modules above; the
public export surface (`addOperation`, `moveOperation`, …) is
unchanged, so the test suite still imports the same barrel.

---

## Batch 3 — Renderer (`renderer/`) · ~3,540 lines

Turns a `Circuit` into SVG. Reads the data layer, never mutates it.
Independent of `actions/` and `editor/`, so it can be reviewed right
after the data layer if you prefer — but it's placed here because
`sqore.ts` (batch 5) ties it together with the editor.

| order | file                                                                                   | lines | what to check                                                                                           |
| ----- | -------------------------------------------------------------------------------------- | ----- | ------------------------------------------------------------------------------------------------------- |
| 9     | [renderer/constants.ts](./renderer/constants.ts)                                       | 49    | gate sizes, paddings, SVG namespace — reference data                                                    |
| 10    | [renderer/gateRenderData.ts](./renderer/gateRenderData.ts)                             | 100   | the render-data shape the formatters consume                                                            |
| 11    | [renderer/layoutMap.ts](./renderer/layoutMap.ts)                                       | 82    | `LayoutMap` value type — the geometry handed to the editor so dropzones use the same numbers as the SVG |
| 12    | [renderer/process.ts](./renderer/process.ts)                                           | 835   | the layout pass: positions every gate, builds the `LayoutMap`. The core of the renderer                 |
| 13    | [renderer/formatters/formatUtils.ts](./renderer/formatters/formatUtils.ts)             | 229   | shared SVG-building helpers                                                                             |
| 14    | [renderer/formatters/registerFormatter.ts](./renderer/formatters/registerFormatter.ts) | 130   | wire/register rendering                                                                                 |
| 15    | [renderer/formatters/inputFormatter.ts](./renderer/formatters/inputFormatter.ts)       | 262   | gate-input/parameter rendering                                                                          |
| 16    | [renderer/formatters/gateFormatter.ts](./renderer/formatters/gateFormatter.ts)         | 933   | gate-shape SVG generation. Large; the visual surface                                                    |

**Focus:** Is the layout pass ([process.ts](./renderer/process.ts)) the
single source of geometry, with the editor consuming `LayoutMap`
rather than re-reading rendered SVG attributes? Confirm the renderer
takes no dependency on `actions/` or `editor/`.

Supporting: [angleExpression.ts](./angleExpression.ts) (133) — angle
parsing used by parameterized-gate input; read it when
[inputFormatter.ts](./renderer/formatters/inputFormatter.ts) references it.

---

## Batch 4 — Editor / view layer (`editor/`) · ~3,400 lines

DOM glue: turns pointer/keyboard events into action calls and builds
the editor chrome. Everything here depends on batches 1–3.

Read the shared scaffolding first, then the controllers:

| order | file                                                                                   | lines | what to check                                                                                                                                     |
| ----- | -------------------------------------------------------------------------------------- | ----- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| 17    | [editor/controllers/interactionContext.ts](./editor/controllers/interactionContext.ts) | 57    | `InteractionContext` — the shared dependency bundle every controller receives (`model`, `interaction`, `renderFn`, …). Read before any controller |
| 18    | [editor/installEditor.ts](./editor/installEditor.ts)                                   | 65    | the one-call editor bootstrap from `sqore.ts`; orchestrates the four install steps                                                                |
| 19    | [editor/shell.ts](./editor/shell.ts)                                                   | 112   | DOM shell: wrapper, toolbox panel, empty-circuit hint                                                                                             |
| 20    | [editor/toolboxGates.ts](./editor/toolboxGates.ts)                                     | 63    | gate templates                                                                                                                                    |
| 21    | [editor/toolbox.ts](./editor/toolbox.ts)                                               | 192   | toolbox element + optional Run button                                                                                                             |
| 22    | [editor/standaloneRenderData.ts](./editor/standaloneRenderData.ts)                     | 102   | `toRenderData` for ghosts / toolbox icons                                                                                                         |
| 23    | [editor/draggable.ts](./editor/draggable.ts)                                           | 863   | `createDropzones`, ghost helpers, wire-dropzone factory. Big; the overlay machinery                                                               |
| 24    | [editor/prompts.ts](./editor/prompts.ts)                                               | 259   | confirm-prompt primitive + gate-specific delete/move confirm flows                                                                                |
| 25    | [editor/contextMenu.ts](./editor/contextMenu.ts)                                       | 380   | right-click menu                                                                                                                                  |
| 26    | [editor/events.ts](./editor/events.ts)                                                 | 214   | `CircuitEvents`: builds the `InteractionContext`, owns + disposes the controllers, fires `modelReady`                                             |

Then the controllers (each translates one input family into actions):

| order | file                                                                                     | lines | what to check                                                                                                                |
| ----- | ---------------------------------------------------------------------------------------- | ----- | ---------------------------------------------------------------------------------------------------------------------------- |
| 27    | [editor/controllers/scrollController.ts](./editor/controllers/scrollController.ts)       | 88    | `enableAutoScroll`, shared by gate + qubit drags                                                                             |
| 28    | [editor/controllers/keyboardController.ts](./editor/controllers/keyboardController.ts)   | 55    | Ctrl-toggle move/copy mode                                                                                                   |
| 29    | [editor/controllers/selectionController.ts](./editor/controllers/selectionController.ts) | 119   | host mousedown + context-menu attach                                                                                         |
| 30    | [editor/controllers/qubitController.ts](./editor/controllers/qubitController.ts)         | 148   | qubit-label drag + remove-with-confirm                                                                                       |
| 31    | [editor/controllers/dragController.ts](./editor/controllers/dragController.ts)           | 997   | **the second giant.** Gate-drag, toolbox-drag, dropzone commit, document-mouseup, add/remove-control. The busiest controller |

**Focus:** Are the controllers genuinely independent — each
reads/writes the shared `model`/`interaction` and re-renders via the
same `renderFn`, with no controller-to-controller coupling? Trace the
"drag H from toolbox" example in [ARCHITECTURE.md](./ARCHITECTURE.md)
against [dragController.ts](./editor/controllers/dragController.ts) +
[draggable.ts](./editor/draggable.ts) to confirm cleanup
(`resetTransient`, ghost teardown, listener removal) on every exit path.

---

## Batch 5 — Entrypoint (`sqore.ts`, `utils.ts`, `index.ts`) · ~1,850 lines

The glue that ties data → renderer → editor together. Read last in the
main subsystem because it references everything above.

| order | file                   | lines | what to check                                                                                                                                                                                 |
| ----- | ---------------------- | ----- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 33    | [utils.ts](./utils.ts) | 861   | shared helpers: the `findOperation` location-walking family, register helpers, DOM lookups. Referenced everywhere                                                                             |
| 34    | [sqore.ts](./sqore.ts) | 931   | the `Sqore` entrypoint. `draw()` → `renderCircuit()`: deep-copy, ID assignment, default-expansion passes, `viewState` overrides, layout pass, then `installEditor`. The render/re-render hook |
| 35    | [index.ts](./index.ts) | 57    | public API barrel — confirm only the intended surface is exported                                                                                                                             |

**Focus:** [sqore.ts](./sqore.ts) is where the lifetimes meet. Verify
the deep-copy-on-render isolation (host circuit never mutated), the
order of default-expansion vs. `viewState` overrides, and that
`renderCircuit` is correctly reused as the editor's re-render hook.

---

## Batch 6 — State visualization (`state-viz/`) · ~1,460 lines

A parallel subsystem (the state panel), loosely coupled to the editor
via the `modelReady` event. Optional / review independently — nothing
in batches 1–5 depends on it.

| order | file                                                                   | lines | what to check                              |
| ----- | ---------------------------------------------------------------------- | ----- | ------------------------------------------ |
| 36    | [state-viz/worker/index.ts](./state-viz/worker/index.ts)               | 18    | worker entry                               |
| 37    | [state-viz/worker/stateVizPrep.ts](./state-viz/worker/stateVizPrep.ts) | 152   | circuit → compute-input prep               |
| 38    | [state-viz/worker/stateCompute.ts](./state-viz/worker/stateCompute.ts) | 260   | the state computation                      |
| 39    | [state-viz/stateVizController.ts](./state-viz/stateVizController.ts)   | 285   | panel controller; listens for `modelReady` |
| 40    | [state-viz/stateViz.ts](./state-viz/stateViz.ts)                       | 748   | the panel UI/rendering                     |

**Focus:** Confirm the coupling to the editor is only the documented
event (`qsharp:circuit:modelReady`) and that compute runs off the main
thread in the worker.

---

## Suggested checkpoints

The heaviest clusters (the `actions/` core and `dragController.ts`) are
natural stopping points. A reasonable cadence:

1. After **Batch 2** — you've seen the entire pure core (data +
   actions). This is the part the test suite you just reviewed
   exercises most heavily; a good place to pause and cross-check tests.
2. After **Batch 4** — the whole editor interaction model is in your
   head; trace one end-to-end flow before moving on.
3. After **Batch 5** — the main subsystem is complete. Batch 6
   (state-viz) can be a separate sitting.

## Reference docs while reviewing

- [ARCHITECTURE.md](./ARCHITECTURE.md) — structure, layering rules,
  end-to-end flow walkthroughs (read first).
- [circuitTargets.bench.md](../../test/circuit-editor/circuitTargets.bench.md)
  — the decision record for the eager `.targets` cache that
  [circuitActions.ts](./actions/circuitActions.ts) maintains.
- [CIRCUIT_EDITOR_TODO.md](./CIRCUIT_EDITOR_TODO.md) /
  [ROADMAP.md](./ROADMAP.md) — outstanding work and design follow-ups.
