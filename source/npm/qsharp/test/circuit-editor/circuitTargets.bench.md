# Circuit `targets` benchmark results

Harness: [circuitTargets.bench.mjs](./circuitTargets.bench.mjs). Captured
with `node test/circuit-editor/circuitTargets.bench.mjs <label>` from
[source/npm/qsharp/](../../).

This file is the **decision artifact** for keeping the eager-cache
design for group `.targets`. A pure-derived alternative was
investigated end-to-end and rejected; see the "Decision" section
below before re-litigating, and
[`CIRCUIT_EDITOR_TODO.md`](../../ux/circuit-vis/CIRCUIT_EDITOR_TODO.md)
sections D6 (rejection narrative) and D7 (planned
`refreshAncestorTargets` utility) for the design follow-up.

## Designs under test

- **eager cache** (kept): group ops carry an authoritative
  `.targets` union of all descendant register sets. Mutators
  (`moveOperation` and friends) walk the ancestor chain and re-run
  `getChildTargets` on every affected group. Readers
  (`getMinMaxRegIdx`, `getOperationRegisters`) read the cached
  field directly — O(1) per op.

- **pure derived** (rejected): group ops have no authoritative
  `.targets`; readers descend `children` when the op has one.
  Mutators do nothing extra. The on-disk format is preserved by
  recomputing group `.targets` once at save time inside
  `Sqore.minimizeOperation`.

## Columns

| column   | meaning                                        |
| -------- | ---------------------------------------------- |
| `ops`    | total operations including descendants         |
| `grps`   | number of group operations                     |
| `render` | median `draw()` call                           |
| `r p95`  | p95 `draw()`                                   |
| `mutate` | median `moveOperation` on a deeply-nested leaf |
| `m p95`  | p95 `moveOperation`                            |

Render iterations: 10 (warmup 2). Mutate iterations: 100 (warmup 3).
JSDOM in Node 22 on win32-x64. Single-machine, no isolation against
background load — treat absolute numbers as noisy but the
**eager-vs-derived ratios** as the actual signal.

## Runs

### eager-cache (2026-05-22, kept)

Today's shipped design. Full ancestor-refresh cascade in
`moveOperation` + per-rung refresh in `_pruneEmptyAncestors`. Group
`.targets` is the source of truth for the renderer's wire-span
queries; the action layer maintains it on every mutation.

| scenario      | ops  | grps | render   | r p95    | mutate   | m p95    |
| ------------- | ---- | ---- | -------- | -------- | -------- | -------- |
| tiny flat     | 51   | 1    | 15.77 ms | 19.44 ms | 105.4 us | 212.2 us |
| small nested  | 274  | 58   | 52.82 ms | 65.86 ms | 432.7 us | 854.4 us |
| medium flat   | 768  | 67   | 103 ms   | 127 ms   | 691.7 us | 1.49 ms  |
| medium nested | 1896 | 349  | 334 ms   | 531 ms   | 2.00 ms  | 3.10 ms  |
| large flat    | 3105 | 221  | 426 ms   | 477 ms   | 2.92 ms  | 5.04 ms  |
| large nested  | 7385 | 1077 | 1221 ms  | 1295 ms  | 8.21 ms  | 12.39 ms |

### pure-derived (2026-05-22, rejected — see "Decision" below)

Numbers captured against a draft implementation that removed group
`.targets` from the action layer's hot path. Readers
(`getMinMaxRegIdx` in [`utils.ts`](../../ux/circuit-vis/utils.ts) and
the resolver's `_getMinMaxRegIdx` shim) walked the subtree at every
call. The on-disk format was restored by a single recompute pass
inside [`Sqore.minimizeOperation`](../../ux/circuit-vis/sqore.ts).
Tests were all green and snapshots byte-identical — the design was
functionally complete, then rejected on the basis of these numbers
plus the semantic-clarity argument below.

| scenario      | ops  | grps | render   | r p95    | mutate   | m p95    |
| ------------- | ---- | ---- | -------- | -------- | -------- | -------- |
| tiny flat     | 51   | 1    | 26.76 ms | 39.72 ms | 145.4 us | 366.7 us |
| small nested  | 274  | 58   | 95.98 ms | 165 ms   | 1.04 ms  | 2.38 ms  |
| medium flat   | 768  | 67   | 262 ms   | 365 ms   | 1.49 ms  | 5.26 ms  |
| medium nested | 1896 | 349  | 620 ms   | 788 ms   | 6.31 ms  | 8.54 ms  |
| large flat    | 3105 | 221  | 1048 ms  | 1363 ms  | 9.63 ms  | 13.14 ms |
| large nested  | 7385 | 1077 | 2712 ms  | 3006 ms  | 23.85 ms | 31.66 ms |

## Comparison (pure-derived ÷ eager-cache)

| scenario      | render | r p95  | mutate | m p95  |
| ------------- | ------ | ------ | ------ | ------ |
| tiny flat     | 1.70 × | 2.04 × | 1.38 × | 1.73 × |
| small nested  | 1.82 × | 2.51 × | 2.41 × | 2.78 × |
| medium flat   | 2.54 × | 2.87 × | 2.15 × | 3.53 × |
| medium nested | 1.86 × | 1.48 × | 3.15 × | 2.75 × |
| large flat    | 2.46 × | 2.86 × | 3.30 × | 2.61 × |
| large nested  | 2.22 × | 2.32 × | 2.91 × | 2.55 × |

Pure-derived is materially slower in both directions, ~1.4–3.3× across
the table. The cost source is the subtree walk inside `getMinMaxRegIdx`:
every renderer and resolver call that previously took O(1) (read a
cached field) becomes O(descendant count). On the `large nested`
scenario (1077 groups, 7385 total ops) the renderer would pay roughly
an extra ~1500 ms per full redraw. The renderer runs on every
keystroke during drag/drop, so this isn't invisible.

## Decision

**Kept the eager cache.** Three reasons, in priority order:

1. **Performance cost is real.** The comparison table above.
   The eager cache buys real speed in the renderer and resolver
   hot paths.

2. **Semantic clarity gets worse, not better, under
   pure-derived.** That model leaves `.targets` populated
   everywhere — JSON schema, file format, every op in memory —
   and ignored by the runtime. The first reader who hits
   `op.targets` and trusts it gets a surprise. The save-time
   recompute inside `Sqore.minimizeOperation` is a fragile
   invariant easy to forget when adding a new save path.
   Eager-cache keeps `.targets` authoritative and the contract
   clean.

3. **The bugs that motivated the investigation are fixable in
   the eager model.** The exercise diagnosed them precisely:
   cascade refresh ordering (refresh-before-mutate vs.
   refresh-after-mutate), the `getChildTargets` strip-`result`
   bug that silently dropped classical-control refs on every
   refresh, and the empty-prune sweep needing to run before the
   parent refresh. None of those required a data-model change.

A hybrid was considered and rejected: per-render memoization on
a pure-derived field would claw back render perf but adds yet
another invariant ("cache valid for the duration of one draw")
and doesn't help mutate at all. More moving parts for less
benefit than keeping the cache authoritative.

## What we kept from the experiment

The investigation wasn't wasted; several artifacts ship as
standalone improvements on the eager-cache baseline:

- This benchmark + `bench.md` — the decision artifact.
- Snapshot harness for `if-else.qs` and `conditionals.qs`,
  which exercises classical-control rendering that was
  previously uncovered.
- The 12 new/rewritten tests in the
  [`circuit-actions/`](./circuit-actions/) suite and
  [`dropzones.test.mjs`](./dropzones.test.mjs) that lock down
  extend-cascade and overlap-split behavior. Their assertions
  read direct `.targets` (the eager-cache contract).
- `_isOperationEmpty` extraction and `_pruneEmptyAncestors`
  structure improvements in the action layer.
- Tighter doc comments on `getMinMaxRegIdx` and
  `getOperationRegisters` describing the authoritative
  `.targets` contract.
