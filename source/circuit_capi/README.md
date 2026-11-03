---
title: Experimental circuit C interface
description: Standalone circuit construction, inspection and checked ownership.
---

## Current surface

The library exposes dense and structured circuits, graph construction and inspection, native outputs, graph decision callbacks, and native pass pipelines through checked C declarations. C, Go/cgo, and Fortran consumers exercise the generated header and standalone library.

The [circuit API notebooks](../../samples/notebooks/circuit_api/README.md) provide
C, Go, and Fortran circuit-building tutorials with binary snapshot handoff to
Python for explicit CPU/GPU simulation. They use Python kernels to compile and
run standalone native clients; no Python runtime is embedded in those clients.

| Capability                                     | Entry points                                                                                     |
| ---------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| Circuit creation, clone, dimensions and phase  | `qdk_circuit_new`, `qdk_circuit_clone`, `qdk_circuit_info`, `qdk_circuit_set_global_phase`       |
| Fixed named registers                          | `qdk_circuit_from_registers`, `qdk_circuit_register`                                             |
| All 32 standard gates and metadata             | `qdk_gate_info`, `qdk_circuit_append` with `QDK_OPERATION_STANDARD`                              |
| Reset, barrier and delay                       | `qdk_circuit_append` with the corresponding operation kind                                       |
| Custom and unitary storage payloads            | `qdk_circuit_append`; storage does not imply compiler or graph support                           |
| Reservation and native batches                 | `qdk_circuit_reserve`, `qdk_circuit_extend_standard`, `qdk_circuit_extend`                       |
| Circuit-owned parameter declarations           | `qdk_circuit_parameter`, `qdk_parameter_id`, `qdk_parameter_free`                                |
| Symbolic arithmetic and full binding           | `qdk_circuit_angle`, `qdk_circuit_parameters`, `qdk_circuit_bind`                                |
| Stored symbolic expression inspection          | `qdk_instruction_angle_expression`, `qdk_angle_expression_nodes`, `qdk_circuit_parameter_at`     |
| Typed runtime expressions and variables        | `qdk_circuit_expression`, `qdk_circuit_variable`, expression traversal                           |
| Existing runtime variable inspection           | `qdk_circuit_variable_at`, `qdk_variable_info`                                                   |
| Structured construction                        | `qdk_circuit_begin`, `qdk_circuit_for_begin`, `qdk_switch_case`, builder finish/cancel/otherwise |
| Read-only blocks and control records           | `qdk_builder_block`, `qdk_instruction_block`, block and control queries                          |
| Exact snapshots, QASM/QIR and display          | `qdk_circuit_import`, `qdk_circuit_export`                                                       |
| Independent copied instructions                | `qdk_circuit_instruction`, `qdk_instruction_info`, typed operand/angle/matrix queries            |
| Graph construction and declarations            | `qdk_graph_builder_*`, `qdk_graph_type_*`, `qdk_graph_angle_*`                                   |
| Independent graph records and verification     | `qdk_graph_record`, typed region/node/port queries, `qdk_graph_edges`, `qdk_graph_verify`        |
| Graph crossings, partial binding and snapshots | `qdk_circuit_to_graph`, `qdk_graph_to_circuit`, `qdk_graph_bind`, `qdk_graph_import/export`      |
| Diagnostics and output ownership               | `qdk_diagnostic_*`, `qdk_bytes_free`, owning handle release functions                            |

Initialize unused `QdkOperation` fields to zero. Standard gate tags are explicit C identifiers, not Rust discriminants or estimator instruction IDs. Parameter handles must belong to the receiving circuit. Native batches accept parameter-free, non-measurement gates on a closed circuit and report the accepted prefix and failing index instead of implying rollback.

Copied records can outlive their circuits. `qdk_instruction_parameter` and `qdk_instruction_angle_expression` validate the original live circuit before creating a new dependent handle at an angle index. A record from a source circuit cannot be used with its clone: obtain a record from the clone itself. A wrong angle variant returns `QDK_WRONG_VARIANT`; an invalid angle index returns `QDK_INVALID_ARGUMENT`. Matrix inspection rejects non-matrix variants.

Stored arithmetic remains inspectable after import or cloning without retaining construction handles. Pass the recovered expression to `qdk_angle_expression_nodes` for a copied, root-last tree; recover its parameter names with `qdk_circuit_parameter_at` and `qdk_parameter_name`. Runtime loop metadata and expression nodes expose variable IDs: pass these to `qdk_circuit_variable_at` on the same circuit, then use `qdk_variable_info` for the existing name and type. Lookup does not declare another variable, and an unknown ID returns `QDK_INVALID_ARGUMENT`. Numeric IDs alone carry no cross-circuit identity.

Recovered handles follow the same dependent-release and callback-scope rules as construction handles. Copy-out queries report required capacity without partial writes. Writable output buffers, including diagnostic destinations, need not be initialized; only the returned element count is written, and spare capacity is untouched.

Nested construction names its current parent builder explicitly. Finished bodies cannot be edited or finished twice. Cancelled constructs are not published. Closed child handles may remain readable, but they prevent parent release until freed. Compile-time loop parameters and runtime loop variables remain different handle types.

Snapshots use the same typed schema-1 document as Python, including registers and exact IEEE floating-point bits. Use `QDK_FORMAT_CIRCUIT_BINARY` (7) for packed Cap'n Proto or `QDK_FORMAT_CIRCUIT_SEXPR` (9) for a readable S-expression envelope. Import/export buffers are length-delimited bytes, not NUL-terminated strings. Decoding limits nesting and binary traversal; callers must bound untrusted circuit input size. This spike has no legacy reader, converter or historical-format compatibility.

QASM flattens register names. Compiler outputs retain native target-profile checks and unsupported-operation diagnostics. `QDK_FORMAT_TEXT` and diagram JSON remain display formats, not executable snapshot formats.

Python uses the [shared Rust facade](../circuit_api/src/lib.rs) directly; this library has no Python runtime dependency. The interface is experimental and makes no stable ABI compatibility promise.

The [experimental Go binding](bindings/go/README.md) exposes Go errors, owned byte
slices, explicit circuit lifetimes, and callback-scoped feedback construction.
It is a reusable module, staged with the matching native library and header, and
used by the Go notebook. Its initial API is intentionally smaller than the C ABI.

The [experimental Fortran binding](bindings/fortran/README.md) provides derived
circuit/body types, returned errors, checked alias lifetimes, and explicit
cloning. Its module and C adapter are staged as source and compiled with the
consumer's compiler; compiler-specific `.mod` files are not SDK artifacts.
C clients continue to use the generated public header and native library directly.

## Graph operations

Graph builders expose the existing native local checks. Finishing spends the builder and returns a graph even when whole-graph verification would report diagnostics. Call `qdk_graph_verify` to obtain an independent diagnostic list; a successful query with a nonzero count means the graph is not verified. Free the builder separately, whether finished or abandoned.

Region, node and port records are independent copies and may outlive their graph. Their enumeration positions are not IDs: `qdk_graph_record` selects by position in ascending local ID order. Record IDs remain local to the source graph. Types, named graph angles, diagnostics and raised-circuit origins are also independently owned. Builders copy type and angle inputs; no graph object retains a caller's type/angle handle.

Typed queries expose nested types, symbols, effects (including unknown versus named resources), payloads, origins and region interface bindings. Tags are explicit C constants, not Rust discriminants. Initialize unused descriptor fields to zero. `QdkOptionalId.present` distinguishes absence from ID zero. Wrong-record and absent-payload queries return `QDK_WRONG_VARIANT`.

Graph angles use parameter names rather than circuit-owned handles. `qdk_graph_bind` returns a new graph, preserves node/port/edge identity, and leaves unnamed parameters unbound. This differs from full circuit binding. Graph snapshots use typed schema 1 with a canonical uncompressed payload digest, a 16 MiB encoded-input cap and a separate 64 MiB expanded-message cap. Export with `QDK_FORMAT_GRAPH_BINARY` (8) or `QDK_FORMAT_GRAPH_SEXPR` (10); `qdk_graph_import` recognizes either envelope. These are QDK-native documents, not HUGR files. Exporting an unverified graph does not make it importable: the decoder still verifies it. The [format contract](../compiler/qsc_snapshot/README.md) defines the typed records, text grammar and digest scopes.

Symbolic angles and supported conditionals cross between circuits and graphs. Loops and unsupported foreign operations retain named refusals. Graph raising returns positional origins for top-level instructions and does not recover named registers. Graph QIR uses the direct native graph compiler; graph QASM first raises to a circuit. Neither route promises support for every construct a graph builder can store.

## Graph callbacks

`qdk_graph_pass_new` retains a synchronous callback plus a `uintptr_t` context token. Successful registration transfers destructor responsibility to the library; failure leaves the context caller-owned. Callbacks and their context must support sequential use on the calling thread, including a different thread from registration. The adapter asserts no unsafe Rust thread traits.

`qdk_graph_pass_run` uses native matching, patches or fragments. A callback runs once per matching node, not once per graph node. Match strings last only for that callback. Return NULL to keep a match, or create an independent decision with `qdk_pass_decision_new`. Returned decisions and diagnostics transfer to the library on callback return, including failure. Rewrite passes accept keep/replace/remove; expansion passes accept keep/expand. The native engine remains responsible for budgets, verification and outcomes.

Callback failure produces no output graph or report and leaves the input graph unchanged. It does not roll back external callback side effects. Active input graph/pass release and recursive execution are refused. Translate foreign errors and panics into a nonzero status and optional owned diagnostic; neither callbacks nor context destructors may unwind across the C ABI.

`qdk_pass_result_info` reports applications, pending work, callback count and the native outcome. Oscillation rule names remain available as owned bytes. The library destroys a successfully registered context exactly once when its last owning registration is released, after all invocations.

## Circuit callbacks and pipelines

`qdk_pipeline_new` selects built-ins and the native optimization level. Register circuit callbacks at `QDK_ANCHOR_CIRCUIT_BUILT` or `QDK_ANCHOR_CIRCUIT_RAISED`; register graph callbacks at `QDK_ANCHOR_GRAPH_LOWERED` or `QDK_ANCHOR_GRAPH_REWRITTEN`. Native anchor order wins over registration order, which breaks ties within an anchor. A wrong-stage registration fails without adopting its context.

Circuit callbacks receive a borrowed working circuit and callback-duration scope. They cannot free or retain either. Every dependent created from the working circuit is callback-scoped, including parameters, expressions, variables, builders, loop indices, and recovered block/condition/parameter handles. Explicit child release unregisters it; the adapter disposes remaining descendants child-first on success, failure and contained unwinding. It never finishes or publishes an unfinished body during cleanup. Returning success with unfinished construction becomes callback failure.

Return success without an offer to select the edited working circuit. To replace it, call `qdk_callback_offer` with an independently owned, closed, unpoisoned, inactive and dependent-free circuit. Acceptance immediately clears the caller's slot. Rejection, including `InUse`, `Borrowed`, or a second offer's `AlreadySet`, leaves ownership with the caller. The adapter consumes an accepted replacement on success or frees it once on subsequent failure. Independent closed clones and copied records can outlive the callback.

`qdk_pipeline_run` uses the native scheduler, keeps the source circuit unchanged, and refuses active input/pipeline release or reentry. Results provide independent circuit/graph copies, executed and changed passes, diagnostics, analysis computations and total foreign invocations (circuit callbacks plus graph matches). Native graph crossings do not restore named registers. Free results and pipelines independently; successfully registered contexts remain alive until pipeline release.

## Build and tests

From the repository root:

```sh
python3 source/circuit_capi/build.py --test
```

The stdlib-only driver builds the debug library and stages a local SDK under
Cargo's resolved `target/circuit-capi/debug/`: the generated header in `include/`,
a copy of the shared library in `lib/`, the reusable Go module in
`bindings/go/`, and Fortran module/adapter sources in `bindings/fortran/`.
It verifies the header against the tracked version and runs all
C11 and Go/cgo tests, including the reusable binding tests from the staged SDK.
It honors `CARGO_TARGET_DIR`, including paths with spaces, and prints the actual
header/library/binding paths. C tests use warnings-as-errors; Go uses
`CGO_ENABLED=1` and `GOEXPERIMENT=cgocheck2`. Missing tools and failed checks exit
nonzero. Go is required only for `--test`, not ordinary workspace or Python builds.

Run the Fortran examples alone or together with C/Go:

```sh
FC=flang python3 source/circuit_capi/build.py --test-fortran
FC=flang python3 source/circuit_capi/build.py --test --test-fortran
python3 -m unittest discover -s source/circuit_capi/tests -p test_build.py
```

`--test-fortran` also checks header freshness and runs the reusable Fortran tests
from the staged SDK, in addition to the ABI fixture tests. It uses `FC` when set;
otherwise it looks for `flang`, `flang-new`, then `gfortran`. An invalid explicit
`FC` fails rather than falling back or skipping tests. Both native test modes use
a GCC/Clang-compatible `CC` (default `cc`). Compiled Fortran modules, objects, and
executables stay in separate test-build directories under Cargo's staging root,
not in the source SDK binding directories. Neither Fortran nor Go is required
for build-only commands.

Build only, or run the separate Rust boundary tests:

```sh
python3 source/circuit_capi/build.py
cargo build -p qdk_circuit_capi
cargo test -p qdk_circuit_capi
```

The crate produces a native shared library and generates `qdk_circuit.h` with the locked `cbindgen` dependency. Cargo's `--message-format=json` output reports the library filenames and the build script's `out_dir`, which contains the header. Both live under Cargo's resolved target directory, including when `CARGO_TARGET_DIR` is set. Ordinary Cargo and build-only commands do not update source files.

The [tracked header](includes/qdk_circuit.h) is a generated reference for reviewing API changes. Rust declarations and [cbindgen configuration](cbindgen.toml) remain authoritative. Never hand-edit the reference; refresh it explicitly and commit the reviewed diff with the corresponding source changes:

```sh
python3 source/circuit_capi/build.py --update-header
python3 source/circuit_capi/build.py --check-header
```

The check fails on missing or stale references without writing them. Native consumer tests imply the same check and compile against the freshly generated staged header. Updating cannot be combined with checks or consumer tests. No separate `cbindgen` CLI installation is needed; its `--verify` mode is not used because it rewrites stale output before reporting a difference.

`cargo test` runs the Rust boundary tests, not the C executables. The [C tests](tests/c/) contain compiled examples of the complete surface, including [graph callbacks](tests/c/pass.c) and [pipeline ownership](tests/c/pipeline.c). The [Go ABI tests](tests/go/circuit_test.go) use a [test-harness cgo shim](tests/go/shim.go) and [C trampolines](tests/go/shim.c); that harness is separate from the [reusable Go binding](bindings/go/README.md). Retained Go state in the ABI harness uses `runtime/cgo.NewHandle` integer cookies and is deleted exactly once; callback panic recovery returns a diagnostic rather than unwinding into Rust.

The [Fortran ABI tests](tests/fortran/circuit_test.f90) retain their low-level
[`ISO_C_BINDING` declarations](tests/fortran/qdk_circuit_bindings.f90) and
[C adapter](tests/fortran/shim.c) to exercise explicit native ownership and fixed
QIR fixtures. They are separate from the [reusable Fortran binding](bindings/fortran/README.md)
used by the notebook. Both adapters range-check signed inputs rather than assuming
portable Fortran equivalents for C unsigned fields.

Runtime validation currently covers macOS arm64, including Flang. No Linux/Windows runtime matrix or release packaging is promised.

On macOS, with `jq` and a C compiler installed, run all C executables from the repository root:

```bash
(
  set -euo pipefail
  build=$(cargo build -p qdk_circuit_capi --message-format=json)
  header_dir=$(printf '%s\n' "$build" | jq -r '
    select(.reason == "build-script-executed"
      and (.package_id | contains("qdk_circuit_capi"))) | .out_dir')
  library=$(printf '%s\n' "$build" | jq -r '
    select(.reason == "compiler-artifact"
      and .target.name == "qdk_circuit_capi")
    | .filenames[] | select(endswith(".dylib"))')
  lib_dir=$(dirname "$library")
  mkdir -p "$lib_dir/circuit-capi-tests"
  for source in source/circuit_capi/tests/c/*.c; do
    executable="$lib_dir/circuit-capi-tests/$(basename "$source" .c)"
    cc -std=c11 -Wall -Wextra -Werror -I "$header_dir" \
      "$source" "$library" -Wl,-rpath,"$lib_dir" -o "$executable"
    DYLD_LIBRARY_PATH="$lib_dir" "$executable"
  done
)
```

The Python snapshot suite contains an optional cross-language parity test. Set `QDK_CIRCUIT_CAPI_LIBRARY` to the freshly built library's absolute path and run `test_circuit_snapshot.py` from the package's tests directory against a rebuilt Python wheel. Without that explicit library path, this one test is reported as skipped; ordinary Python builds do not gain a C-library prerequisite.

## Adaptive QIR snapshots

The Go, Fortran, and [Python conversion tests](../qdk_package/tests/test_circuit_conversion.py) independently construct two qubits and two classical bits: H on qubit 0, measure into bit 0, conditionally apply X to qubit 1 when bit 0 is One, then measure qubit 1 into bit 1. This is measurement feedback, not a constant condition.

Each language compares its complete compilation output against the same profile-specific fixture:

| Profile               | C tag                     | Snapshot                                        |
| --------------------- | ------------------------- | ----------------------------------------------- |
| Full Adaptive, QIR v2 | `QDK_PROFILE_ADAPTIVE`    | [adaptive.ll](tests/fixtures/adaptive.ll)       |
| Adaptive_RI, QIR v1   | `QDK_PROFILE_ADAPTIVE_RI` | [adaptive_ri.ll](tests/fixtures/adaptive_ri.ll) |

Python uses `QuantumCircuit.to_qir(TargetProfile.Adaptive)` or `TargetProfile.Adaptive_RI` through its direct Rust adapter, not ctypes. Run `python -m pytest test_circuit_conversion.py test_circuit_usage_doc.py` from `source/qdk_package/tests` with a matching rebuilt Python package. These tests need neither the standalone library environment variable nor a QIR runner.

Snapshot checks compare every UTF-8 byte, including declarations, profile attributes, module flags, and final newline. Missing files and mismatches fail; normal tests never update expectations. For an intentional compiler-output change, explicitly capture each profile's output from the same circuit on the current build, review the entire `.ll` diff (including measurement-dependent branching and both recorded results), and rerun all three language suites. Do not trim or normalize the output, or substitute one profile's snapshot for the other. These are compiler regression tests, not QIR execution or ABI-compatibility proofs.

## Ownership and errors

- The library that allocates an object or byte buffer must release it.
- Release any previously owned value before reusing an output slot. Output functions do not free the slot's old contents.
- Creation/query output slots may be uninitialized. Release functions require initialized owning slots or buffer descriptors.
- Release functions clear owning slots. Releasing a NULL circuit, diagnostic or buffer is a no-op.
- Strings carry explicit byte lengths and use UTF-8. Output buffers are not NUL-terminated.
- A non-NULL diagnostic output receives an owned error on failure and NULL on success. Free its previous value before reusing the slot.
- Normal failures leave newly produced owning outputs empty. Buffer-capacity failures report the required size without partial writes.
- Circuit release refuses active, borrowed or still-referenced handles. Release dependent parameter handles before their circuit; an `InUse` refusal leaves both the owning slot and circuit unchanged.
- Objects cannot be accessed concurrently. Pointer validity, allocation extent and disjoint output slots remain caller obligations; dangling pointers and double frees are not recoverable input errors.
- An unwinding Rust panic poisons every possibly mutated owner and its dependents. Subsequent semantic access is refused; child-first drop-only teardown remains available. Abort-mode failures, allocation aborts, and foreign undefined behavior are outside that guarantee.
