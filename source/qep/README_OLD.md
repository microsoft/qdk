THIS IS THE OLD DESIGN
THIS IS NOT HOW WE ARE GOING TO IMPLEMENT IT
WE ARE CURRENTLY IN PROCESS OF REWRITING THE WAY IT'S IMPLEMENTED HERE
TO NEW DESIGN DESCRIBED at README.md

# Quantum Execution Profiler (QEP)

## Scope

QEP executes trusted Quantum Intermediate Representation (QIR) programs in Rust.
An LLVM-backed runner dispatches intrinsic calls through a shared backend
interface. The three backends ignore gates, count calls, or simulate a sparse
quantum state.

QIR runtime support and profile validation are partial. The counting backend
counts intrinsic calls; it does not model hardware timing or estimate
fault-tolerant resources.

## Goals

- Run the same QIR program with different backends.
- Keep LLVM parsing, control flow, and runtime ABI handling outside backends.
- Separate issuing a measurement from resolving its classical value.
- Preserve structured output and report backend failures to the caller.
- Support inspection through intrinsic counts and sparse wavefunction snapshots.

QEP does not support dynamic resource management, asynchronous execution,
hardware scheduling, or sandboxing untrusted programs.

## Architecture

```text
Q# source
    |
    | QDK compiler (external to the runner; used in integration tests)
    v
QIR file (.ll or .bc)
    |
    v
run_qir
    |-- Parse and validate supported constructs
    |-- Bind runtime callbacks and instrument error propagation
    |-- Execute native code through the LLVM JIT
    |
    +--> Per-invocation ExecutionState
             |-- Resource bounds and result initialization tracking
             |-- Output records and first execution error
             |
             +--> QirBackend
                      |-- NoopBackend
                      |-- CountingBackend --> NoopBackend
                      +-- SparseSimBackend --> SparseStateSim
```

The caller owns the backend; `run_qir` borrows it mutably for one execution.
Generated wrappers pass an execution-state pointer to native callbacks instead
of relying on global or thread-local backend state. Independent backends can
execute concurrently.

### Source organization

| Component | Responsibility |
| --- | --- |
| [src/lib.rs](src/lib.rs) | Public exports and the `QirBackend` contract |
| [src/runner.rs](src/runner.rs) | File loading, JIT execution, validation, and error instrumentation |
| [src/runner/runtime.rs](src/runner/runtime.rs) | Native callbacks, ABI bindings, invocation state, and output recording |
| [src/backends/](src/backends/) | Backend implementations |
| [src/tests/backends/](src/tests/backends/) | Tests organized by backend |
| [src/runner/tests.rs](src/runner/tests.rs) | Runner and runtime integration tests |

## Backend contract

`QirBackend` uses `usize` IDs for qubits and result slots. Rotation angles are in
radians. Measurement values use `false` for zero and `true` for one. Methods
return `Result<_, String>` to report failures.

| Category | Methods |
| --- | --- |
| Controlled gates | `ccx`, `cx`, `cy`, `cz` |
| Rotations | `rx`, `ry`, `rz`, `rxx`, `ryy`, `rzz` |
| Single-qubit gates | `h`, `s`, `sadj`, `sx`, `t`, `tadj`, `x`, `y`, `z` |
| Exchange | `swap` |
| Measurement and reset | `mz`, `mresetz`, `reset` |
| Classical result access | `read_result`, `write_result` |

### Measurement semantics

`mz` and `mresetz` issue a measurement into a result slot; `read_result` resolves
its value. All current backends complete measurements synchronously. The
separation lets a future backend defer completion until a read, then wait or
advance modeled time.

- Repeated reads return the stored value without measuring again.
- Measuring or writing into an existing slot replaces its previous value.
- `mresetz` stores the pre-reset measurement value.
- `reset` does not write a result slot.
- Reading a slot that has never been written is an error.

The runner rejects reads of slots that have not been measured or written during
the current invocation, even if the backend holds a value from an earlier run.
Recording a result as output also resolves it via `read_result`.

## Backends

### NoopBackend

[NoopBackend](src/backends/noop.rs) ignores gates and resets; it does not maintain
a quantum state. Each measurement samples and stores an independent, uniformly
random bit. Reading results does not consume randomness.

Use it to test execution without simulating the circuit. Its random outcomes
do not predict the circuit's results.

### CountingBackend

[CountingBackend](src/backends/counting.rs) forwards operations to `NoopBackend`
and records calls in `CountingBackendCounts`. The public `counts()` method
returns a shared reference to the counters.

- Each `QirBackend` method has an individual counter.
- `measurements` is the sum of `mz` and `mresetz` calls.
- Counters record attempted calls, including failed result reads.
- `mresetz` leaves the `mz` and `reset` fields unchanged.
- Counts accumulate over the lifetime of the backend.

Counts describe backend calls, not distinct LLVM symbol names. For example,
the supported `cnot` and `cx` QIR aliases both dispatch to `cx`. Runtime
initialization and array/tuple output markers are not backend intrinsics and do
not have counters. Result output recording does increment `read_result`.

For adaptive programs, the no-op backend's random measurement outcomes determine
which path executes. Counts cover that path only, not every possible branch.

### SparseSimBackend

[SparseSimBackend](src/backends/sparse_sim.rs) uses the QDK `SparseStateSim` to
evolve the quantum state. It uses native simulator operations where available,
multi-controlled operations for controlled gates, and gate decompositions for
`rxx`, `ryy`, `rzz`, and `sx`.

Measurements collapse the state and store the sampled value by result ID.
Reset measures the target and applies `X` when the result is one. Classical
result writes update only result storage, not the wavefunction.

The inspection API is:

```rust
pub fn wavefunction(&mut self) -> Vec<(BigUint, Complex64)>
```

Calling it flushes queued simulator operations without measuring the state. The
returned snapshot owns its basis-state indices and complex amplitudes, sorted
by index, and omits the qubit count. Snapshot cost and simulation memory grow
with the represented state. State-vector simulation can still require
exponential resources in the worst case.

## QIR execution model

### Input and entry point

`run_qir` accepts textual LLVM IR (`.ll`) or LLVM bitcode (`.bc`). LLVM 20 parses
both QIR 1 typed-pointer and QIR 2 opaque-pointer inputs.

A supported module must have exactly one defined entry point:

- Marked with `entry_point` or the legacy `EntryPoint` attribute.
- Taking no arguments.
- Returning `void` or `i64`.
- Declaring both `required_num_qubits` and `required_num_results`.

The runner supports direct internal function calls, branching, loops, and
adaptive control flow. It checks the ABI and supported constructs but does not
fully validate conformance to a named QIR profile.

### Static resource management

The two resource-count attributes define exclusive upper bounds for static IDs:

```text
0 <= qubit ID  < required_num_qubits
0 <= result ID < required_num_results
```

These IDs are encoded in pointer values, which must not be dereferenced as host
addresses. Callbacks check the declared bounds before passing IDs to the
backend. The result count specifies storage capacity. A program can measure
more times than this count by overwriting slots.

The runner rejects dynamic qubit/result management flags set to true and does
not support runtime allocation or release calls.

The runner knows the declared bounds, but `QirBackend` has no allocation or
initialization hook to receive them. `SparseSimBackend` therefore allocates
simulator qubits on demand through the highest ID used by a gate or measurement. Accessing ID 5
allocates IDs 0 through 5 if needed. These qubits remain allocated for the
backend's lifetime. This is an internal allocation strategy, not dynamic QIR
resource management.

### Initialization and repeated execution

Each invocation gets fresh output storage, error state, and result-written
markers. The backend keeps its state between invocations.

The `__quantum__rt__initialize` callback is a no-op: it does not restore an
existing sparse simulator to the zero state, as the QIR profiles expect.
Create a fresh backend for independent executions that need a fresh quantum
state. Reuse a backend only when you intend to preserve its state and counters.

### Binding and failure propagation

The runner checks external declarations against a list of supported
names and LLVM signatures. It generates wrappers that pass execution state to
the backend callbacks. Supported runtime operations are initialization, result
read/write, and array/tuple/result/bool/int/double output recording.

Unsupported externals, including LLVM intrinsics, are rejected rather than
resolved from the host. Additional checks reject reserved symbol definitions,
unsupported globals, indirect calls, variadic functions, non-C calling
conventions, and exception-handling call instructions.

Callbacks capture backend errors and unwinding panics before they cross the C
ABI. The runner inserts failure checks after calls so errors stop execution,
including inside nested functions. Inserting these checks splits basic blocks,
so the runner rewrites PHI predecessors and removes function and call-site
attributes that could invalidate the checks.

On failure, `run_qir` returns an error and leaves any backend changes in place.
These checks cannot handle process aborts, memory faults, or nonterminating QIR.

## Output representation

`RunOutput` contains:

- `records: Vec<OutputRecord>` in emission order.
- `return_value: Option<i64>`: `None` for a void entry point, otherwise the
  returned integer. This is separate from the recorded quantum-program output.

Each record contains an `OutputValue` and an optional UTF-8 label. Values are
`Array(u64)`, `Tuple(u64)`, `Result(bool)`, `Bool(bool)`, `Int(i64)`, or
`Double(f64)`.

Array and tuple records are structural markers followed by their elements.
Returning two measurement results as an array therefore produces three records:

```text
Array(2)
Result(first measurement)
Result(second measurement)
```

The runner keeps the stream flat. It does not rebuild nested values or validate
the complete output structure.

## Safety boundary

`run_qir` is unsafe because it JIT-compiles QIR and executes it as native machine
code. LLVM verification and resource bounds checks cannot sandbox that code.

The caller must supply trusted QIR with valid memory accesses and ABI usage.
Non-null output labels must point to valid, null-terminated UTF-8 strings.
Programs must not retain callbacks or invoke them asynchronously. Execution is
synchronous and requires a 64-bit host.

QEP does not depend on the separate QIR Runner project.

## Build and testing

Building QEP requires LLVM 20 development libraries; there is no optional
`runner` feature. Set `LLVM_SYS_201_PREFIX` if the build cannot find your LLVM
installation. Default features are disabled for `qdk_simulators`, so QEP does
not request GPU support. Tests use the Q# compiler as a development dependency.
The runner itself loads QIR files, not Q# source.

From the repository root:

```sh
cargo test -p qep
cargo test -p qep tests::backends::sparse_sim
cargo fmt -p qep -- --check
```

Current test coverage includes:

- Intrinsic dispatch, aliases, argument preservation, and backend counters.
- No-op measurement sampling and stable result reads.
- Sparse-simulator intrinsic smoke tests, Bell-state correlation, reset
  behavior, and sparse wavefunction inspection.
- Q# compilation to QIR, temporary-file execution, and counting-backend checks.
- Text and bitcode loading, structured output, and writable results.
- Adaptive loops, internal calls, PHI rewriting, and error/panic propagation.
- Invalid resource IDs, unwritten results, unsupported modules, invocation
  isolation, and concurrent execution with independent backends.

## Open design questions

The following changes remain unimplemented:

- Pass declared resource counts to backends and define how initialization and
  reset affect existing state. This could replace lazy allocation.
- Support dynamic resources with allocation/release bindings, pointer ownership
  tracking, and backend lifecycle operations.
- Add deferred measurement resolution and modeled scheduling for timing-aware
  profiling, using the existing separation between issuing and reading results.
- Support more runtime calls and profile checks while retaining the native-code
  safety contract.