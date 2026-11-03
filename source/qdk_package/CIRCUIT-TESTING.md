# Circuit API testing strategy

Tests for the circuit API fit the repository's existing structure rather than sitting beside it. This document names where each kind of test lives, which ones run by default, and how the two measurements that are not ordinary assertions are instrumented.

## Where tests live

| Kind                                          | Location                                                                                         | Runner                                            |
| --------------------------------------------- | ------------------------------------------------------------------------------------------------ | ------------------------------------------------- |
| Rust unit and integration tests for the store | Alongside the store crate, following the crate conventions already used under `source/compiler/` | `cargo test` from the repository root             |
| Shared native facade seams                    | [source/circuit_api/src/tests.rs](../circuit_api/src/tests.rs)                                   | `cargo test -p qdk_circuit_api`                   |
| Python unit tests for the public surface      | `source/qdk_package/tests/`                                                                      | `pytest`, invoked by the build entry point        |
| Python tests requiring Qiskit or Cirq         | `source/qdk_package/tests-integration/`                                                          | `pytest`, invoked only with `--integration-tests` |
| Throughput benchmarks                         | `source/qdk_package/benchmarks/`                                                                 | Run directly; not collected by `pytest`           |

The split between the two Python directories is not stylistic. Everything in `tests/` runs on every build, so it must depend only on the package itself. Anything needing a third-party quantum framework belongs in `tests-integration/`, which installs its own requirements and is opt-in.

The shared-facade tests cover document/register fidelity, angle representation, builder cancellation and close order, diagram conversion, and preserved graph refusals. Existing store/codegen/graph suites remain the owners of engine semantics. The Python control-flow suite also checks that body and case builders retain their parents after caller variables are dropped.

Rebuild the Python extension before running extraction regressions. Run pytest from `source/qdk_package/tests`, not the source package directory, to avoid shadowing the installed wheel. Run the API-surface checker with Python's safe-path option (`python -P ../check_api_surface.py` from the tests directory) for the same reason.

## Standalone native consumers

Run `python3 source/circuit_capi/build.py --test` from the repository root to build the standalone library and generated header, compile/run the C11 consumers, and run Go/cgo with `GOEXPERIMENT=cgocheck2`. The driver resolves Cargo artifacts, including `CARGO_TARGET_DIR` and paths with spaces. Missing tools fail explicitly. This opt-in gate adds no Go prerequisite to the root build.

The [C suite](../circuit_capi/tests/c/) covers marshalling, dependent lifetimes, graph records and crossings, native pass decisions and pipeline callbacks. The [Go suite](../circuit_capi/tests/go/) covers owned copies, diagnostics, symbolic/dynamic construction, graph output, retained integer callback tokens surviving GC, panic translation and exactly-once cleanup. These check language boundaries rather than duplicate the existing engine regression suites. Rust FFI tests count live handles for callback failure, contained unwinding and checked replacement transfer.

The snapshot suite's `test_c_api_roundtrip_preserves_complete_python_document` uses the actual library when `QDK_CIRCUIT_CAPI_LIBRARY` is set to its absolute path. Enable it for native acceptance validation; without that explicit path, the test reports a skip rather than introducing a native-library dependency for ordinary Python callers.

## What runs by default

`./build.py` runs the Rust tests and the Python unit tests. It does not run integration tests or benchmarks.

Integration tests run only under `--integration-tests`, and they run once per entry in the repository's Qiskit version matrix, currently Qiskit 1.x and Qiskit 2.x. Structural converter tests therefore have to pass against both major versions. Where the two versions differ in a way the converter must handle, the test states which version it targets rather than silently passing on one.

Cirq tests follow the same placement rule. The package already declares Cirq as an optional extra and already has an interop directory under `tests-integration/`.

## Allocation counting

The requirement that appending a standard gate performs no heap allocation cannot be checked with a normal assertion, because allocation is a process-wide property rather than a value the code returns.

The repository already solves this. `source/compiler/qsc/src/bin/memtest.rs` defines an `AllocationCounter` that wraps a `GlobalAlloc`, incrementing an atomic on allocation and decrementing on deallocation, with `reset` and `read` methods. The circuit store reuses that pattern rather than introducing a second approach.

Two constraints govern placement, and the second is easy to miss.

A global allocator can be installed only once per binary, so the counter has to live in a binary that nothing else shares. A dedicated Rust integration test file gets its own test binary, which makes it a valid home and allows the measurement to be asserted rather than printed. The existing `memtest` binaries print a number instead, which suits a reporting tool but leaves the check to whoever reads the output.

That test binary must also contain exactly one test. The counter is process-global, and the test harness runs tests in parallel threads, so a second test allocates while the first is measuring and those allocations land in the count. Serializing with a mutex is not sufficient, because the harness allocates outside any lock the tests can take. Measured during the store work: three parallel tests reported 29 allocations where the true count was zero, and adding a mutex only reduced that to 5. Running the cases sequentially inside a single test reports the true zero.

The assertion is bounded: appending a run of standard gates allocates nothing beyond amortized vector growth. Growth is separated from per-gate cost by reserving capacity first. Interning adds a second condition, since the first appearance of an operand pattern stores a key, so the measured run warms its patterns before counting and the steady-state claim is about repeated patterns.

## FFI crossing counting

The requirement that a Python pass performs at most one FFI crossing per match, plus a fixed per-run setup, also needs instrumentation. It does not need native instrumentation.

A crossing in the direction that matters is an invocation of the user's Python callable from Rust. That is directly observable in Python: the test wraps its pass callable in a counting proxy, and every call to the proxy is exactly one crossing. Calls in the other direction are made by the test itself and are therefore already known.

This keeps the counter out of the shipped native module. An atomic counter on the boundary would add cost to the exact path the design is trying to make fast, and a build feature to gate it would mean testing a differently compiled artifact from the one that ships.

The test holds match count constant while varying graph size, and asserts the crossing count does not change. That distinguishes a per-match bound from a per-node one, which a single-size measurement cannot do.

## Benchmarks

Benchmarks live in `source/qdk_package/benchmarks/` and are not part of the default test run, because throughput numbers are sensitive to machine load and would make the build flaky.

Two properties keep the numbers meaningful. Benchmarks report best, median, and p95 over a stated sample count, and any derived target uses the best sample, since benchmark noise only ever makes a run slower. They also record the host, interpreter, and dependency versions with the result, because a number without its conditions cannot be compared against a later one.

`bench_graph_memory.py` measures per-gate memory of the graph representation. It runs one gate count per subprocess, because resident memory does not fall back reliably after a large allocation and a single process would charge each size for its predecessors.
