# qsc_fir_transforms

The production FIR-to-FIR rewrite pipeline. It runs after FIR lowering and before downstream consumers such as partial evaluation and backend code generation, producing FIR that is semantically equivalent to the input but easier for those consumers to handle.

## What to know before diving in

- **It is one pipeline, not a toolbox of independent passes.** The passes are ordered and assume each other's output. Several intermediate states deliberately violate FIR invariants that later passes restore, so running a pass in isolation or reordering passes is generally unsound. Treat `run_pipeline_with_diagnostics` (and the staged `run_pipeline_to_with_diagnostics`) as the only supported way to invoke them.

- **Rewrites are entry-reachability-driven.** Most passes inspect what is reachable from the package entry expression and only mutate that. UDT erasure is the main exception: it is still reachability-scoped but works at package granularity across the reachable package closure (target package plus any package with an entry-reachable callable; unreachable packages are left alone).

- **One `PackageAssigners` pool is threaded through the pipeline.** Each package has its own ID space and reuses its own `Assigner` across passes. Allocate copied guards and other synthesized nodes with the owning package's assigner, not the entry package's. The trailing metadata passes only delete nodes or rebuild derived data.

- **Synthesized nodes use the `EMPTY_EXEC_RANGE` sentinel.** New exprs/stmts carry an empty `exec_graph_range`; the final `exec_graph_rebuild` pass consumes that sentinel and recomputes the execution graph.

- **Only consume output when there are no fatal diagnostics.** A failed pipeline can leave the store at an intermediate, invalid state. Warning-only diagnostics are preserved and do not block successful output.

## Pass order

The driver first validates intrinsics, collapses simulatable intrinsics, and clears orphaned nodes. The main rewrite schedule is:

1. `monomorphize` — specialize reachable generic callables to concrete types.
2. `return_unify` — rewrite bodies to single-exit form, removing `Return` nodes while preserving path-local side effects (e.g. qubit release).
3. `cond_normalize` — preserve selection-time conditions before callable analysis and dispatch rewriting.
4. `defunctionalize` — specialize known callable choices and rewrite calls to direct dispatch. Unresolved alternatives remain dynamic rather than being discarded in favor of a known branch.
5. `udt_erase` — replace UDT values and struct expressions with tuple/scalar form across the reachable package closure.
6. `tuple_compare_lower` — lower equality/inequality on non-empty tuples to element-wise scalar comparisons.
7. `tuple_decompose` — decompose eligible tuple-valued locals into scalar fields.
8. `arg_promote` — flatten tuple-valued callable parameters and update call sites.

   Steps 7 and 8 iterate to a fixed point (convergence is guaranteed by a strictly-decreasing measure).

9. `normalize_reachable_call_arg_types` — reconcile call argument types once after the fixed point.
10. `item_dce` — remove unreachable items while retaining pinned callables and their dependencies.
11. `gc_unreachable` — remove orphaned arena nodes across the reachable package closure.
12. `exec_graph_rebuild` — recompute exec-graph metadata from the rewritten FIR.

Invariant checks run after most passes. `run_pipeline_to_with_diagnostics` exposes each stage as a cut point used by tests and (with `PipelineStage::Full` plus pinned callable items) by production codegen.

Within defunctionalization, higher-order and direct-call rewrites share a
children-before-parents traversal. An enclosing call must copy already-rewritten
arguments, not stale argument layouts paired with a newly specialized callee.
If an inner rewrite replaces a capture operand with a new local read, its
dependent direct or higher-order calls wait for fresh capture analysis. Direct calls also wait
when their callee gains control flow that needs normalization. Closure cleanup treats
computed callees as live dependencies, just like call arguments; consuming one
use does not make another invocation disposable.

## Where to look

- `src/lib.rs` — pipeline orchestration, stage cut points, and the cross-pass contracts above.
- One file per pass (`src/monomorphize.rs`, `src/return_unify.rs`, …, `src/exec_graph_rebuild.rs`).
- `src/invariants.rs` — staged structural checks.
- `src/reachability.rs`, `src/walk_utils.rs`, `src/cloner.rs` — shared traversal, use-collection, and deep-cloning helpers.
- `src/pretty.rs` — FIR-to-Q# pretty-printer used by before/after snapshot tests.
- `src/test_utils.rs` — compile-and-run-to-stage helpers (re-exported under the `testutil` feature for external crates).

## Testing

```bash
cargo test -p qsc_fir_transforms                                  # default lane
cargo test -p qsc_fir_transforms --features slow-proptest-tests   # + semantic-equivalence proptests
```

Pass-local unit tests sit next to each pass; `tests/pipeline_integration.rs` drives full-pipeline and per-stage behavior.

Return-normalization semantic tests compare explicit expected values, ordered
quantum-operation and lifetime traces, and receiver output before and after the
Full pipeline. They do not establish QIR support by themselves. The
short-circuit-assignment codegen tests keep qubit ownership in the caller so they
exercise return lowering independently of conditional qubit cleanup. The
resource-owning #3836 case still requires the separate cleanup repair.
