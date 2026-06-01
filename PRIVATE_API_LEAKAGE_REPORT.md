# Private API leakage in `qdk` — analysis & proposed plan

## Background

Auditing docstrings and Sphinx cross-references in the `qdk` package surfaced
a systemic issue: a number of types defined in private modules
(`qdk._native`, `qdk._types`, `qdk._interpreter`, and several `qdk.qre.*`
submodules) appear in the public API surface — as return types, parameter
types, or types referenced in docstrings of public functions and methods.

This causes three concrete problems:

1. **Broken cross-references in the generated docs.** Tools like py2docfx and
   Sphinx can't emit working links to a type that lives at a non-public path,
   so the generated reference pages contain dead `<xref:>` markers.
2. **Inconsistent / undiscoverable API.** Users who follow a return type
   annotation to import it cannot, because the type isn't reachable from any
   stable `qdk.*` path (e.g. `from qdk._native import Circuit` is technically
   importable but is explicitly private).
3. **Static typing breaks.** Type checkers (pyright, mypy) flag references
   to private modules as private-symbol violations when used from user code,
   even when the type is unavoidable because it's the return type of a public
   function.

This document inventories the leakage and proposes a categorized plan to
resolve it.

## Leakage outside `qdk.qre`

### 1. Native types appearing in `qdk.qsharp` and `qdk.openqasm` signatures

The following types are imported from `qdk._native` (the Rust extension) or
`qdk._types` and appear in parameter or return positions of public
functions, but are **not** currently re-exported on any public path:

| Type             | Defined in    | Used by                                                              | Position    |
| ---------------- | ------------- | -------------------------------------------------------------------- | ----------- |
| `Circuit`        | `qdk._native` | return type of `qdk.qsharp.circuit()`                                | return      |
| `QirInputData`   | `qdk._types`  | return type of `qdk.qsharp.compile()` and `qdk.openqasm.compile()`   | return      |
| `Config`         | `qdk._types`  | return type of `qdk.qsharp.init()`                                   | return      |
| `GlobalCallable` | `qdk._native` | `qdk.qsharp.run`, `compile`, `circuit`, `estimate`, `logical_counts` | parameter   |
| `Closure`        | `qdk._native` | `qdk.qsharp.run`, `compile`, `circuit`, `estimate`, `logical_counts` | parameter   |
| `NoiseConfig`    | `qdk._native` | `qdk.qsharp.run`, `qdk.openqasm.run`                                 | parameter   |
| `Output`         | `qdk._native` | callback signatures used by `run` event-saving paths                 | callback    |
| `StateDumpData`  | `qdk._native` | inputs to user-facing `StateDump` construction                       | constructor |
| `CircuitConfig`  | `qdk._native` | configuration object passed internally by `circuit()`                | internal    |
| `Interpreter`    | `qdk._native` | return type of internal `get_interpreter()` helper                   | internal    |

Notes:

- `Circuit`, `QirInputData`, and `Config` are concrete return types of
  public top-level functions. Without a public path, users cannot annotate
  variables that hold these values, and the doc pages that describe
  `circuit()`, `compile()`, and `init()` cannot link their return types.
- `NoiseConfig` is **already** re-exported as `qdk.simulation.NoiseConfig`,
  so it has a public home; the issue is only that `qdk.qsharp.run`'s union
  type annotation references the bare `NoiseConfig` rather than
  `qdk.simulation.NoiseConfig`.
- `GlobalCallable` and `Closure` represent Q# callables and closures
  produced by the interpreter and stored on user-facing callable objects
  (via `__global_callable`). They are part of the contract users see when
  passing a Q#-generated callable back into `run` / `compile` / `circuit`.
- `Output` and `StateDumpData` are wrappers that the user receives in
  callback contexts; they appear in event-saving code paths.
- `CircuitConfig` and `Interpreter` are arguably internal-only and could
  stay private, with the recommendation being to remove them from
  documented signatures rather than promote them.

### 2. Internal types confirmed not to leak

The following types are imported from `qdk._native` or defined in private
modules and are used in internal helper signatures only. They have been
verified not to appear in any user-visible docstring or public signature
and should remain private.

- `TypeIR`, `TypeKind`, `PrimitiveKind`, `UdtValue` (used by [`_context.py`](source/qdk_package/qdk/_context.py) in the dynamic Q#-class generation machinery).
- `CircuitConfig`, `Interpreter` (used by internal helpers `get_interpreter()` and `circuit()`).

## Leakage inside `qdk.qre`

The `qre` module has the most extensive leakage and warrants treatment as a
cohesive design refresh rather than piecemeal patches.

### Categorized by severity

#### Public methods returning or exposing private types

| Public surface                                    | Private type                      |
| ------------------------------------------------- | --------------------------------- |
| `qdk.qre.InstructionSource.__getitem__(id)`       | `_InstructionSourceNodeReference` |
| `qdk.qre.InstructionSource.get(id, default=None)` | `_InstructionSourceNodeReference` |
| `qdk.qre.InstructionSource.nodes`                 | `list[_InstructionSourceNode]`    |
| `qdk.qre.ISATransform.bind(name, node)`           | `_BindingNode`                    |
| `qdk.qre.ISAQuery.__add__(other)`                 | `_SumNode`                        |
| `qdk.qre.ISAQuery.__mul__(other)`                 | `_ProductNode`                    |
| `qdk.qre.TraceQuery` (class)                      | inherits from private `_Node`     |
| `qdk.qre.Application.context()`                   | `_Context`                        |

The `_InstructionSourceNodeReference` case is the clearest leak: it's the
**only** way to reach the child-node traversal API
(`_InstructionSourceNodeReference.__getitem__`,
`_InstructionSourceNodeReference.get`,
`_InstructionSourceNodeReference.instruction`,
`_InstructionSourceNodeReference.transform`), but the type itself isn't
public, so users have to type-erase to `Any` or import from
`qdk.qre._instruction` directly.

#### Private types in public parameter positions

| Method                                   | Parameter type      |
| ---------------------------------------- | ------------------- |
| `qdk.qre.TraceQuery.enumerate(ctx, ...)` | `_Context`          |
| Many `*_to_trace` Cirq adapters          | `_CirqTraceBuilder` |

`_CirqTraceBuilder` lives in [qdk/qre/interop/\_cirq.py](source/qdk_package/qdk/qre/interop/_cirq.py)
and is referenced by ~12 module-level `*_to_trace` functions that are
visible to users registering custom gate translations. It also exposes a
`q_to_id` property typed as the private `_QidToTraceId`.

#### TypeVars not re-exported

| Symbol            | Defined in                                                             | Used in                                                                                                                                                                                                                                          |
| ----------------- | ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `TraceParameters` | [qdk/qre/\_application.py](source/qdk_package/qdk/qre/_application.py) | `Application[TraceParameters]`, `Application.get_trace(parameters: TraceParameters)`, `Application.post_process(parameters: TraceParameters, ...)`, `Application.enumerate_traces_with_parameters(...)` (yields `tuple[TraceParameters, Trace]`) |

Users subclassing `Application` need this TypeVar to write their own
generic specializations, but it's not exported from `qdk.qre`.

#### Public types not re-exported in submodules

These exist publicly at `qdk.qre.X` but are referenced by classes that live
in deeper submodules like `qdk.qre.models` and `qdk.qre.models.factories`.
py2docfx renders xrefs relative to the type's module, so links break:

| Type from `qdk.qre`    | Used in                                                                                      |
| ---------------------- | -------------------------------------------------------------------------------------------- |
| `Instruction`          | `InstructionSource.add_node(instruction: Instruction, ...)`                                  |
| `ISA`, `ISAContext`    | `Litinski19Factory.provided_isa(self, impl_isa: ISA, ctx: ISAContext)` and similar factories |
| `EstimationTableEntry` | `EstimationTable.add_column(function: Callable[[EstimationTableEntry], Any])`                |
| `ConstraintBound`      | `qdk.qre.constraint(error_rate: Optional[ConstraintBound] = None)`                           |

Note that `Instruction` is **not even in `qdk.qre.__all__`**, so it's
effectively unreachable except via `qdk.qre._qre.Instruction` (private).

#### Internal types confirmed not to leak

The following private types are used in internal positions only and have
been verified not to appear in any public signature. They should remain
private.

- `_Entry`, `_Protocol` in [qdk/qre/models/factories/\_litinski.py](source/qdk_package/qdk/qre/models/factories/_litinski.py) and [qdk/qre/models/factories/\_cultivation.py](source/qdk_package/qdk/qre/models/factories/_cultivation.py) (distillation-table representations).
- `_ComponentQuery` (used internally by `ISATransform.q()`, which returns the public `ISAQuery` base type).
- `_PSSPC`, `_LatticeSurgery` (private aliases of Rust types wrapped by the public `PSSPC` and `LatticeSurgery` classes).

## Proposed plan

We recommend routing all currently-private leaked types through a pair of
new "internal but visible" namespaces:

- **`qdk.internal`** for non-qre types that are unavoidably exposed in the
  public surface but are not part of the supported API.
- **`qdk.qre.internal`** for the analogous qre-specific types. `qre` has
  enough internal surface area that a separate namespace keeps the
  top-level `qdk.internal` from being dominated by qre concerns.

These namespaces are explicitly internal — their module docstrings make
clear that types defined or re-exported there are not part of the supported
public API and may change in any release without notice — but they are
reachable from a stable import path. This fixes all three problems in
[Background](#background): doc-gen tools can emit working xrefs, users who
follow type annotations land on a clearly-labeled page, and type checkers
no longer flag references as private-module accesses.

Types that users genuinely instantiate and configure (e.g. `PauliNoise`,
`NoiseConfig`, `EstimatorParams`) remain at their existing public paths.
The `internal` namespaces are reserved for types users only ever receive
from the API or pass through as opaque values.

### Placement strategy

For each leaked type, choose one of:

- **Promote to a fully public path** (`qdk.X`, `qdk.qsharp.X`, `qdk.qre.X`, etc.) if users will construct or configure instances of the type directly.
- **Re-export under `qdk.internal` or `qdk.qre.internal`** if users will only encounter the type through annotations or as a return value, but the type still needs to be reachable for documentation and type-checking purposes.
- **Leave private** if the type does not appear in any user-facing signature or docstring.

The two `internal` modules should:

- Have a top-of-file docstring that explicitly identifies them as internal and warns against direct use.
- Be re-export shims only — canonical definitions stay in their existing private modules.
- Not be advertised in the top-level `qdk` package overview docstring.

### Tier 1 — User-blocking. Do as part of next minor release.

Each item here represents a case where the user **cannot** reach or
correctly annotate a type that appears in a public signature.

Non-qre (re-export under `qdk.internal`):

1. **Re-export `Circuit`** from `qdk._native` under `qdk.internal`. Return type of `qdk.qsharp.circuit()`.
2. **Re-export `QirInputData`** from `qdk._types` under `qdk.internal`. Return type of `qdk.qsharp.compile()` and `qdk.openqasm.compile()`.
3. **Re-export `Config`** from `qdk._types` under `qdk.internal`. Return type of `qdk.qsharp.init()`.
4. **Re-export `GlobalCallable` and `Closure`** under `qdk.internal`. These appear in the union type of `entry_expr` for every public `run`/`compile`/`circuit`/`estimate`/`logical_counts` function.
5. **Re-export `Output` and `StateDumpData`** under `qdk.internal`. These appear in user-facing callback signatures (the `on_save_events` path for `run`).

qre (re-export under `qdk.qre.internal`):

6. **Re-export `_InstructionSourceNodeReference` as `qdk.qre.internal.InstructionSourceNodeReference`.** Only way to traverse instruction-source children, returned from public methods.
7. **Re-export `_InstructionSourceNode` as `qdk.qre.internal.InstructionSourceNode`.** Exposed as the element type of the public `InstructionSource.nodes` attribute.
8. **Re-export `_Context` from `qdk.qre._application` as `qdk.qre.internal.ApplicationContext`.** Return type of `Application.context()` and an input to `TraceQuery.enumerate`.
9. **Re-export `_BindingNode` from `qdk.qre._isa_enumeration` as `qdk.qre.internal.BindingNode`.** Return type of `ISATransform.bind`.
10. **Re-export `_SumNode` and `_ProductNode` as `qdk.qre.internal.ISASumNode` and `qdk.qre.internal.ISAProductNode`.** Return types of `ISAQuery.__add__` and `ISAQuery.__mul__`. Alternative: widen the return-type annotations to the public `ISAQuery` base, since callers rarely need the concrete subtype.
11. **Address the `_Node` base of public `TraceQuery`.** Either re-export as `qdk.qre.internal.TraceNode`, or if `TraceQuery` is the only public consumer, drop the inheritance and inline the abstract `enumerate` method on `TraceQuery`.
12. **Re-export `Instruction` as `qdk.qre.internal.Instruction`.** Currently leaks via `InstructionSource.add_node(instruction: Instruction, ...)` and is not in any `__all__`.

Public surface (no internal namespace needed):

13. **Add `TraceParameters` (TypeVar) to `qdk.qre.__all__`.** Users subclassing `Application` legitimately use this in their own annotations, so it belongs on the public surface, not in `internal`.

After Tier 1, every type appearing in a public function signature has a
reachable, doc-linkable home.

#### Opacity — implemented

Both `qdk.internal` and `qdk.qre.internal` now use a
`typing.TYPE_CHECKING` guard to implement the opacity model:

- **Type-checking time:** Each exported name resolves to a
  `typing.Protocol` that exposes only the stable method subset (e.g.
  `Circuit` exposes `.json()`, `__repr__`, `__str__`; `QirInputData`
  exposes `._repr_qir_()`, `._name`, `__str__()` only; `Instruction`
  exposes read-only properties and query methods but not mutation
  methods like `set_source` / `set_property`).
- **Runtime:** The `else` branch re-exports the real class, so existing
  code continues to work unchanged.
- **No `isinstance` checks** are performed on these types anywhere in
  the public surface (verified).

This means users get autocomplete and doc links for the stable surface
only, while other methods are clearly implementation details. Internal
code continues to import directly from `_native` / `_types` / private
submodules and is unaffected.

#### Jupyter / notebook integration — testing notes

`Config`, `Output`, and `StateDumpData` have special roles in the Jupyter
notebook experience. While the proposed re-exports should not change
runtime behavior (the Jupyter display protocol is duck-typed via
`_repr_mimebundle_`, `_repr_markdown_`, etc., and re-exporting does not
change class identity), the following scenarios should be verified after
the Tier 1 changes land:

- **`Config` MIME bundle round-trip.** Calling `qdk.qsharp.init()` in a
  notebook cell must still emit an `application/x.qsharp-config` MIME
  output item that the VS Code extension can parse. The extension reads
  the raw JSON bytes from cell output — it does not import or
  `isinstance`-check `Config` — but we should confirm the data still
  flows correctly.
- **`Output` display in `%%qsharp` cells.** The `%%qsharp` cell magic
  calls `IPython.display.display(output)` on `Output` objects. IPython
  renders them via `Output._repr_markdown_()`. Verify that state dumps,
  messages, and matrix outputs still render correctly after the change.
- **`StateDumpData` in `save_events` path.** When `save_events=True`,
  `Context.eval()` and `Context.run()` extract `StateDumpData` via
  `output.state_dump()` and wrap it in `StateDump`. Confirm that
  `StateDump._repr_markdown_()` still works and that the `check_eq` /
  `as_dense_state` methods are unaffected.

Note: `qsharp_widgets` does **not** import any of these types. Its
`Circuit` widget accepts any object with a `.json()` method (duck-typed),
and its only `qdk` import is a lazy `from qdk import qsharp` inside
`Histogram.run()`. The widgets package will not be affected by these
changes.

### Tier 2 — Discoverability. Schedule for the same release if budget allows.

These items eliminate broken xrefs and improve doc-gen output. The
underlying functionality is already reachable.

1. **Re-export `ISA` and `ISAContext` from `qdk.qre.models` and `qdk.qre.models.factories`** so that signature renderings inside those submodules resolve. These types are already public at `qdk.qre.X`; the submodule re-exports are a doc-gen accommodation, not a new API.
2. **Re-export `_CirqTraceBuilder` as `qdk.qre.internal.CirqTraceBuilder` and `_QidToTraceId` as `qdk.qre.internal.QidToTraceId`.** `_CirqTraceBuilder` is the parameter type of ~12 user-facing trace-builder functions; `_QidToTraceId` is the return type of its public `q_to_id` property.
3. **Make `qdk.qsharp.run`'s `noise:` parameter annotation use `qdk.simulation.NoiseConfig` instead of the bare `NoiseConfig`.** `NoiseConfig` is already publicly exposed, just under a different submodule name.
4. **Audit `qdk.qsharp` and `qdk.openqasm` `__all__` for missing entries** that appear in signature renderings.

## Process recommendations

### Enforce the existing public/private convention

The codebase follows the standard Python convention: names (and modules)
starting with an underscore are private and not part of the supported API
surface. This convention is purely advisory — Python itself enforces
nothing, and nothing in the type system or our current build prevents a
private-named type from appearing in the signature or docstring of a
public function or method. Every leak documented in this report is an
instance of that pattern: the private types are correctly named, but they
reach the public surface via return types, parameter types, and docstring
references.

The fix is to mechanically enforce the existing convention at build time.

### Add a CI lint

A lightweight check that walks every `__all__` symbol, inspects its
signature annotations and docstring `:type:` / `:rtype:` / `:param:`
references, and fails if any of them name an underscore-prefixed symbol
(or a symbol whose nearest enclosing module starts with underscore).

This would have caught all the leakage in this report at code-review time.

Sample heuristic (pseudocode):

```python
for module in walk_qdk_modules():
    for name in module.__all__:
        sym = getattr(module, name)
        for annotation in collect_annotations(sym):
            if annotation_names_private_symbol(annotation):
                report_violation(module, name, annotation)
```

Run as part of `./build.py` so violations land in the same gate that
prevents the PR from merging.

## Summary

- Placement strategy: route currently-private leaked types through new `qdk.internal` and `qdk.qre.internal` namespaces, clearly labeled internal but reachable for docs and type-checking. Promote to fully public paths only the types users genuinely instantiate.
- Tier 1 list (user-blocking): 13 changes (5 non-qre, 7 qre, 1 public).
- Tier 2 list (discoverability): 4 changes.
- Recommended process: enforce the existing public/private boundary with a CI lint in `./build.py`.
