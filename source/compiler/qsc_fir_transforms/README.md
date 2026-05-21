# Overview

`qsc_fir_transforms` owns the production FIR-to-FIR rewrite schedule that runs after FIR lowering and before downstream consumers such as partial evaluation and backend code generation.

The passes in this crate are ordered and staged as one pipeline. They are not intended to be individually sound in arbitrary combinations. Some intermediate results are only valid because later passes restore the structural guarantees that downstream code expects.

Most rewrites are entry-reachability-driven. They inspect the code that can be reached from the package entry expression and limit mutation accordingly. The main exception is UDT erasure, which is still reachability-scoped but operates at package granularity within the reachable package closure: it rewrites the target package plus any package that contains an entry-reachable callable, leaves unreachable packages untouched, and resolves UDT definitions from the whole store.

## Public entry point

`run_pipeline_with_diagnostics` is the public production entry point. It runs the full rewrite schedule on one FIR package and returns pipeline diagnostics produced by `return_unify`, `defunctionalize`, or pinned-item validation. Warning-only diagnostics do not block successful `PostAll` output. Fatal diagnostics leave the FIR store at an intermediate state that must not be consumed as successful pipeline output.

`run_pipeline_to_with_diagnostics` exposes the same schedule up to a requested stage. Crate tests use it for stage cut points, and production codegen uses it with `PipelineStage::Full` plus pinned callable items. Pinned items must be existing callables; they are retained through item DCE and included in exec graph rebuild so callable-generation paths can keep using original callable IDs after defunctionalization specializes the entry call.

## Pipeline

The authoritative pass order is:

1. `monomorphize`
2. `return_unify`
3. `defunctionalize`
4. `udt_erase`
5. `tuple_compare_lower`
6. `sroa`
7. `arg_promote`
8. `gc_unreachable`
9. `item_dce`
10. `exec_graph_rebuild`

The passes have the following responsibilities:

1. `monomorphize` specializes reachable generic callables to the concrete types
   used from the entry expression.
2. `return_unify` rewrites callable bodies to a single-exit form by
   eliminating all `ExprKind::Return` nodes while preserving path-local side
   effects such as qubit release calls.
3. `defunctionalize` eliminates callable-valued expressions and rewrites call
   sites to direct callable references where possible.
4. `udt_erase` replaces UDT-typed values and struct expressions in the
   reachable package closure with their pure tuple or scalar representation.
5. `tuple_compare_lower` lowers equality and inequality on non-empty tuples to
   element-wise scalar comparisons.
6. `sroa` iteratively decomposes tuple-valued locals when every use is a field
   access or another decomposable aggregate update.
7. `arg_promote` iteratively decomposes tuple-valued callable parameters and
   updates reachable call sites.
8. `gc_unreachable` tombstones orphaned blocks, stmts, exprs, and pats that
   are no longer reachable from any callable body or entry expression.
9. `item_dce` removes unreachable callable and type items left behind by
    monomorphization and defunctionalization.
10. `exec_graph_rebuild` recomputes exec-graph metadata after earlier passes
   synthesize new FIR nodes, including selected external callable specs that
   UDT erasure structurally mutated.

Invariant checks run after `monomorphize`, `return_unify`, `defunctionalize`,
`udt_erase`, `tuple_compare_lower`, `sroa`, `arg_promote`, and
`gc_unreachable`, then after `item_dce` at the `PostItemDce` cut point. After
exec graph rebuild, the pipeline checks mutated external specs with an
exec-graph-only validator and runs `PostAll` for the full pipeline.

## Module guide

* `src/lib.rs` defines the production schedule, the stage cut points used by
  crate tests, and the shared pipeline contract.
* `src/monomorphize.rs`, `src/return_unify.rs`, `src/defunctionalize.rs`,
  `src/udt_erase.rs`, `src/tuple_compare_lower.rs`, `src/sroa.rs`,
  `src/arg_promote.rs`, `src/gc_unreachable.rs`, `src/item_dce.rs`, and
  `src/exec_graph_rebuild.rs` implement the ordered transform stages.
* `src/invariants.rs` defines the staged structural checks that validate
  intermediate and final pipeline states.
* `src/reachability.rs` computes the entry-reachable callable set shared by
   multiple passes.
* `src/walk_utils.rs` provides traversal and use-collection helpers for
  passes that rewrite FIR in place.
* `src/cloner.rs` provides reusable deep-cloning support for passes that need
  to synthesize FIR while preserving consistent ID remapping.
* `src/pretty.rs` provides a FIR-to-Q# pretty-printer used by before/after
  snapshot tests for pass debugging.
* `src/test_utils.rs` provides crate-local helpers that compile Q# snippets,
  lower them to FIR, and run the authoritative schedule to an intermediate
  stage.

## Transformation shapes

| Pass | Before | After |
|------|--------|-------|
| `monomorphize` | Generic callables with `Ty::Param` and non-empty generic-argument lists | Concrete callables; all `Ty::Param` resolved, generic-argument lists empty |
| `return_unify` | Multiple `ExprKind::Return` nodes in callable bodies, including raw qubit-release return wrappers | Single-exit form; no `Return` nodes remain in reachable code, and path-local releases stay on their original paths |
| `defunctionalize` | Arrow-typed parameters, closures, indirect callable dispatch | Direct dispatch only; no `ExprKind::Closure` or arrow-typed params in reachable code |
| `udt_erase` | `Ty::Udt` values, `ExprKind::Struct`, `Field::Path` in update/assign | Pure tuple or scalar representations; no UDT surface in reachable package closure, with `Field::Path` allowed only for tuple-field reads |
| `tuple_compare_lower` | `BinOp(Eq/Neq)` on non-empty tuple-typed operands | Element-wise scalar `AndL`/`OrL` chains with `Field` extractions |
| `sroa` | Tuple-valued locals used only via field access | Decomposed scalar bindings; tuple binding replaced by per-field `PatKind::Bind` |
| `arg_promote` | Tuple-valued callable parameters | Flattened scalar parameters; call sites pass individual fields |
| `gc_unreachable` | Orphaned arena nodes (blocks, stmts, exprs, pats) from earlier rewrites | Tombstoned entries; only nodes reachable from package items or the entry expression survive |
| `item_dce` | Unreachable callable/type items (original generics, dead closure items) | Items removed from `Package::items`; `gc_unreachable` re-runs if items were deleted |
| `exec_graph_rebuild` | Stale `exec_graph_range` with `EMPTY_EXEC_RANGE` sentinels | Fresh exec graphs rebuilt from the rewritten target FIR tree and selected mutated external specs |

## Testing

The crate uses both pass-local unit tests and end-to-end integration tests.

* Unit tests live next to each pass and focus on localized rewrites,
  invariants, and edge cases.
* `src/invariants/tests.rs` adds mutation-style coverage for staged structural
  guarantees.
* `tests/pipeline_integration.rs` compiles Q# snippets through the full
  pipeline, compares the public `run_pipeline_with_diagnostics` wrapper with
  an explicit pass schedule, and preserves targeted regression cases.
* The integration tests rely on the staged cut-points exposed by
  `run_pipeline_to_with_diagnostics` to assert per-stage behavior.

## Test lanes

The default test lane keeps deterministic tests enabled and excludes the slower
semantic-equivalence proptests:

```bash
cargo test -p qsc_fir_transforms
```

Enable the slower proptest-backed semantic-equivalence suites with the
`slow-proptest-tests` feature:

```bash
cargo test -p qsc_fir_transforms --features slow-proptest-tests
```

### External test helpers

The `testutil` feature re-exports the staged pipeline helpers in
`src/test_utils.rs` so external crates can drive the same compile-and-run-to-stage
flow used by the in-crate tests.
