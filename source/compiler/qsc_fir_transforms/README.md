# qsc_fir_transforms

The production FIR-to-FIR rewrite pipeline. It runs after FIR lowering and before downstream consumers such as partial evaluation and backend code generation, producing FIR that is semantically equivalent to the input but easier for those consumers to handle.

## What to know before diving in

- **It is one pipeline, not a toolbox of independent passes.** The passes are ordered and assume each other's output. Several intermediate states deliberately violate FIR invariants that later passes restore, so running a pass in isolation or reordering passes is generally unsound. Treat `run_pipeline_with_diagnostics` (and the staged `run_pipeline_to_with_diagnostics`) as the only supported way to invoke them.

- **Rewrites are entry-reachability-driven.** Most passes inspect what is reachable from the package entry expression and only mutate that. UDT erasure is the main exception: it is still reachability-scoped but works at package granularity across the reachable package closure (target package plus any package with an entry-reachable callable; unreachable packages are left alone).

- **One `Assigner` is threaded through the whole pipeline.** Every pass that synthesizes FIR nodes allocates fresh IDs from a single shared `Assigner` so IDs never collide across stages. Do not construct a new `Assigner` mid-pipeline. The trailing metadata passes (`gc_unreachable`, `item_dce`, `exec_graph_rebuild`) don't get it because they only tombstone, delete, or rebuild derived data.

- **Synthesized nodes use the `EMPTY_EXEC_RANGE` sentinel.** New exprs/stmts carry an empty `exec_graph_range`; the final `exec_graph_rebuild` pass consumes that sentinel and recomputes the execution graph.

- **Only consume output when there are no fatal diagnostics.** Fatal diagnostics (from `return_unify`, `defunctionalize`, or pinned-item validation) leave the store at an intermediate, invalid state. Warning-only diagnostics are preserved and do not block successful output.

## Pass order

1. `monomorphize` — specialize reachable generic callables to concrete types.
2. `return_unify` — rewrite bodies to single-exit form, removing `Return` nodes while preserving path-local side effects (e.g. qubit release).
3. `defunctionalize` — eliminate callable-valued expressions/closures; rewrite call sites to direct dispatch.
4. `udt_erase` — replace UDT values and struct expressions with tuple/scalar form across the reachable package closure.
5. `tuple_compare_lower` — lower equality/inequality on non-empty tuples to element-wise scalar comparisons.
6. `tuple_decompose` — decompose tuple-valued locals whose uses are all field accesses.
7. `arg_promote` — flatten tuple-valued callable parameters and update call sites.

   Steps 6 and 7 iterate to a fixed point (convergence is guaranteed by a strictly-decreasing measure).

8. `gc_unreachable` — tombstone orphaned arena nodes.
9. `item_dce` — remove unreachable callable/type items; re-run `gc_unreachable` if anything was deleted.
10. `exec_graph_rebuild` — recompute exec-graph metadata from the rewritten FIR.

Invariant checks run after most passes. `run_pipeline_to_with_diagnostics` exposes each stage as a cut point used by tests and (with `PipelineStage::Full` plus pinned callable items) by production codegen.

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
