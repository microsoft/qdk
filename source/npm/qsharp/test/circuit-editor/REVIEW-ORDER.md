# Circuit Editor Test Review Order

Suggested **bottom-up by architectural layer** review order for the
24 test files under `source/npm/qsharp/test/circuit-editor/`.

Each layer's tests reuse fixtures and conventions established by the
layer below, so reading them in order means each new file's helpers
are already familiar.

Baseline: **412/412 tests passing**.

---

## Stage 1 — Pure data, no JSDOM

Warm-up; tiny files, easy to verify.

| #   | File                                                         | LOC | Tests | Scope                                                                    |
| --- | ------------------------------------------------------------ | --- | ----- | ------------------------------------------------------------------------ |
| 1   | [location.test.mjs](./location.test.mjs)                     | 106 | 14    | `Location` value-type. Defines the address syntax every other file uses. |
| 2   | [angleExpression.test.mjs](./angleExpression.test.mjs)       | 218 | 18    | Isolated helpers, no dependencies.                                       |
| 3   | [findOperation.test.mjs](./findOperation.test.mjs)           | 162 | 15    | Location-walking helpers; uses `Location`.                               |
| 4   | [viewState.test.mjs](./viewState.test.mjs)                   | 375 | 18    | Per-session view-preference layer.                                       |
| 5   | [interactionActions.test.mjs](./interactionActions.test.mjs) | 178 | 10    | `InteractionState` mutations.                                            |

## Stage 2 — Data layer (Model contract)

| #   | File                                             | LOC | Tests | Scope                                                                          |
| --- | ------------------------------------------------ | --- | ----- | ------------------------------------------------------------------------------ |
| 6   | [circuitModel.test.mjs](./circuitModel.test.mjs) | 376 | 17    | `CircuitModel` invariants. Sets up the fixture style used in the action tests. |

## Stage 3 — Pure-helper utilities (used by both actions and rendering)

| #   | File                               | LOC | Tests | Scope                                              |
| --- | ---------------------------------- | --- | ----- | -------------------------------------------------- |
| 7   | [utils.test.mjs](./utils.test.mjs) | 560 | 32    | 5 grouped helpers; pure data + a small DOM corner. |

## Stage 4 — Action layer (the bulk of behavioral coverage)

> **Save a long sitting for this one** — `circuitActions.test.mjs` is
> ~40% of the suite by line count.

| #   | File                                                 | LOC  | Tests | Scope                                           |
| --- | ---------------------------------------------------- | ---- | ----- | ----------------------------------------------- |
| 8   | [circuitActions.test.mjs](./circuitActions.test.mjs) | 6835 | 126   | The main event. 15 contiguous, themed sections. |

Suggested sub-order within `circuitActions.test.mjs`:

| Lines         | Section                                                    |
| ------------- | ---------------------------------------------------------- |
| L1 – L444     | Bookkeeping + edge cases (light; sets baseline)            |
| L446 – L1354  | `moveOperation` cross-scope + multi-wire rigid motion      |
| L1356 – L1888 | Classical-condition ordering                               |
| L1890 – L3556 | Dropzone & ancestor-refresh cascades + `moveQubit`         |
| L3558 – L5106 | Clone-copy, control add/remove rules, group + control move |
| L5108 – L5848 | View-state stamp + measurement-with-consumers              |
| L5850 – end   | Column-split chapter (4 contiguous sections)               |

## Stage 5 — Rendering/formatting (pure-data outputs, no controllers)

| #   | File                                               | LOC  | Tests | Scope                                         |
| --- | -------------------------------------------------- | ---- | ----- | --------------------------------------------- |
| 9   | [gateFormatter.test.mjs](./gateFormatter.test.mjs) | 396  | 18    | 6 themed helpers.                             |
| 10  | [draggable.test.mjs](./draggable.test.mjs)         | 382  | 14    | Geometry math for dropzones.                  |
| 11  | [dropzones.test.mjs](./dropzones.test.mjs)         | 1232 | 15    | Heavy integration test of dropzone rendering. |

## Stage 6 — Editor primitives & UI surfaces

| #   | File                                                     | LOC | Tests | Scope                          |
| --- | -------------------------------------------------------- | --- | ----- | ------------------------------ |
| 12  | [prompts.test.mjs](./prompts.test.mjs)                   | 222 | 7     | Confirm-dialog primitive.      |
| 13  | [operationPrompts.test.mjs](./operationPrompts.test.mjs) | 730 | 12    | Uses `prompts` + Action layer. |
| 14  | [contextMenu.test.mjs](./contextMenu.test.mjs)           | 542 | 13    | 5 themed sections.             |
| 15  | [toolbox.test.mjs](./toolbox.test.mjs)                   | 207 | 5     | Quick read.                    |
| 16  | [toolboxRunButton.test.mjs](./toolboxRunButton.test.mjs) | 92  | 3     | Quick read.                    |

## Stage 7 — Controllers (small, focused, similar shape)

| #   | File                                                           | LOC  | Tests | Scope                                  |
| --- | -------------------------------------------------------------- | ---- | ----- | -------------------------------------- |
| 17  | [keyboardController.test.mjs](./keyboardController.test.mjs)   | 163  | 6     |                                        |
| 18  | [scrollController.test.mjs](./scrollController.test.mjs)       | 172  | 8     |                                        |
| 19  | [selectionController.test.mjs](./selectionController.test.mjs) | 375  | 13    |                                        |
| 20  | [qubitController.test.mjs](./qubitController.test.mjs)         | 448  | 9     |                                        |
| 21  | [dragController.test.mjs](./dragController.test.mjs)           | 1725 | 28    | Largest controller; 6 themed sections. |

## Stage 8 — Top-level shell

| #   | File                               | LOC | Tests | Scope                                                                                                                                           |
| --- | ---------------------------------- | --- | ----- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| 22  | [sqore.test.mjs](./sqore.test.mjs) | 385 | 11    | `Sqore` outer shell. Reads naturally last because it stitches together `ViewState` (Stage 1) + `CircuitModel` (Stage 2) + `Location` (Stage 1). |

---

## Practical tips

- **Take a break before Stage 4.** `circuitActions.test.mjs` is ~40%
  of the suite by line count and warrants its own session.
- **Skim the file headers first.** Each `// Copyright` block has a
  short summary paragraph that tells you what's in scope and what's
  deliberately not — useful framing before diving into tests.
- **Section dividers freshest in:** `sqore`, `viewState`,
  `qubitController`, `interactionActions`, `dragController`. Review
  the new dividers as you encounter them.

## Running the tests

```pwsh
cd source/npm/qsharp
node --test "test/circuit-editor/*.test.mjs"
```

To run a single file:

```pwsh
node --test test/circuit-editor/circuitActions.test.mjs
```
