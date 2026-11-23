# qsc_fir_transforms

The production FIR-to-FIR rewrite pipeline. It runs after FIR lowering and before partial evaluation and backend code generation. It preserves codegen semantics, including replacing simulation-only intrinsic implementations with their external-call contracts. It is not run before ordinary interpreter simulation.

## What to know before diving in

- **It is one pipeline, not a toolbox of independent passes.** The passes are ordered and assume each other's output. Several intermediate states deliberately violate FIR invariants that later passes restore, so running a pass in isolation or reordering passes is generally unsound. Use the pipeline entry points, including `run_pipeline_with_diagnostics`, `run_pipeline_to_with_diagnostics`, and its shared-budget counterpart, rather than calling individual passes.

- **Rewrites are reachability-driven.** Most passes start from the package entry expression. UDT erasure includes pinned roots and works at package granularity across that reachable closure. Argument promotion includes pinned bodies and dependencies in safety checks and caller rewrites, but preserves pinned root signatures.

- **One `PackageAssigners` pool is threaded through the pipeline.** Each package has its own ID space and reuses its own `Assigner` across passes. Allocate copied guards and other synthesized nodes with the owning package's assigner, not the entry package's. The trailing metadata passes only delete nodes or rebuild derived data.

- **Synthesized nodes use the `EMPTY_EXEC_RANGE` sentinel.** New exprs/stmts carry an empty `exec_graph_range`; the final `exec_graph_rebuild` pass consumes that sentinel and recomputes the execution graph.

- **Only consume output when there are no fatal diagnostics.** A failed pipeline can leave the store at an intermediate, invalid state. Warning-only diagnostics are preserved and do not block successful output.

## Pass order

The driver first validates intrinsics, collapses simulatable intrinsics, and clears orphaned nodes. Their simulation bodies do not contribute calls, effects, or constants to partial evaluation. Built-in intrinsic handling remains separate, and codegen no-op intrinsic calls are lowered immediately after monomorphization. The main rewrite schedule is:

1. `monomorphize` — specialize reachable generic callables to concrete types.
2. `return_unify` — rewrite bodies to single-exit form, removing `Return` nodes while preserving path-local side effects (e.g. qubit release).
3. `cond_normalize` — preserve selection-time conditions before callable analysis and dispatch rewriting.
4. `defunctionalize` — specialize known callable choices and rewrite calls to direct dispatch. Unresolved alternatives remain dynamic rather than being discarded in favor of a known branch.
5. `udt_erase` — replace UDT values and struct expressions with tuple/scalar form across the reachable package closure. Core Complex addition, subtraction, multiplication and unary signs lower to scalar component operations before nominal identity is lost.
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

Complex component lowering currently covers addition, subtraction, multiplication,
unary signs, and the corresponding compound assignments. Division and
exponentiation are not covered by this lowering.

Monomorphization identifies a specialization by the package-qualified source
callable and its concrete generic arguments. Recursive references retain the
generic source identity until those arguments are substituted, so same-type,
mutual, and finite changing-type cycles resolve to the matching concrete targets.
Each codegen preparation allows at most 1024 specializations across all packages,
shared by runtime-instance preparation and the subsequent pipeline.
Types constructed for specializations and each reference's type-argument list
are limited to 4096 type nodes and depth 128, counting a leaf as depth one.
Existing types outside specialization are not subject to these limits.
Substitution is measured before expanded types are constructed. Exceeding a
limit reports `Qdk.Qsc.Monomorphize.SpecializationLimit` or
`Qdk.Qsc.Monomorphize.TypeExpansionLimit` at the original source and stops the
pipeline before downstream consumption. A runtime-terminating program that
continually nests its recursive type arguments can therefore be rejected,
including programs previously accepted by profiles that evaluated those calls
classically. These are specialization limits, not runtime recursion limits or
new backend support for recursive results.

## Runtime callable inputs

Evaluation retains checked type/functor arguments and lexical contexts on callable
values without changing source identities or choosing different simulation bodies.
Concrete global values keep the inline representation; globals with retained or
unbound arguments share a separate instance allocation. Callable metadata therefore
does not enlarge scalar `Value` slots, and concrete post-monomorphization references
do not allocate an instance merely to represent empty type bindings.
Python interop validates instantiated public signatures and context ownership;
it does not infer generic arguments from Python values.

Codegen consumes that evidence in a private FIR store. It uses
`instantiate_runtime_closure` for runtime targets and
`run_pipeline_to_with_specialization_budget` for the following pipeline.
The budget belongs to that codegen store and must not be reused for an independent
compilation.
Parameter discovery is cached by source identity, and matching source/argument
instances share a specialization without merging their captured values or charging
the budget twice. Named local declarations keep their own generic binders.

The ordinary monomorphizer can still derive arguments from statically checked
FIR capture and arrow types. That is distinct from the removed inference from
erased runtime payloads. Nominal runtime values are reconstructed with typed
constructors and keep their identities until normal UDT erasure.

Runtime values containing non-lowerable identities or foreign closures use the
pinned route. It specializes dependencies before signature-preserving rewriting
and invokes the prepared target with runtime values; it does not require the
returned target ID to equal the original interpreter ID.

Callable capability weakening does not change value layout: an adjointable or
controllable operation can populate a less-capable operation binding without
adding tuple wrappers. Assignment checks retain nominal identity, tuple shape,
callable kind and input/output compatibility; they do not allow capability upgrades.

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
