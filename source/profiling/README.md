# Profiling

## Profiling in QDK

Profiling is a resource estimation tool that fully executes a quantum program
against a model of a quantum processing unit (QPU). The model tracks how long
execution takes, how many qubits the program needs and how much error it
accumulates.

Classical computation, loops and measurement-dependent branches run as part of
profiling. The profiler does not maintain a wavefunction, so measurement results
may not be consistent with what a real quantum computer would return. This
matters for programs whose runtime depends on the probability of a measurement
outcome.

We use [Quantum Service Protocol (QSP)](../qsp/README.md) to connect the program
to the model:

- A runner executes the program and sends batches of quantum instructions.
- The profiler processes those instructions as a QSP backend and returns
  measurement results, so the runner can continue execution.
- Adapters between the runner and profiler can decompose instructions into the
  model's native instruction set or translate between measurement models.

The runner and profiler can run in the same process or communicate over streams.
The profiler handles the same instructions whether they come from Q#, QIR or a
native program using QSP.

## Profilers

[Profiler](src/profilers.rs) is a Rust trait that extends QSP's `QuantumBackend`.
A profiler maintains internal state and implements `get_result()`, which returns
a `ProfilerResult` containing resource estimates, temporal profiles and an
optional execution trace.

We currently have two implementations: counting and temporal.

### Counting profiler

[SequentialCountingProfiler](src/profilers/counting.rs) counts instructions and
assumes that they execute sequentially. Runtime is the sum of each instruction's
call count multiplied by its configured duration. Physical qubit usage is logical
qubit usage multiplied by a configured constant.

We use this simple model to count instructions and measure the overhead of
executing a program through different runners. It does not model hardware in
detail.
Only gates specified in `gate_time_sec` appear in the per-operation count and
runtime maps and contribute to the total operation count. Executed gates without
configured timings contribute to qubit usage but have zero runtime and produce
an informational message.

### Temporal profiler

[TemporalProfiler](src/profilers/temporal_profiler.rs) models execution over time.
It schedules native operations on functional units, allowing independent
operations to run in parallel and waiting when a required resource is not
available. It can track error on logical qubits and record time series and an
execution trace. See [TemporalProfiler.md](src/profilers/TemporalProfiler.md) for
the scheduler, model assumptions, configuration and current limitations.

## Models

A QPU model is defined in terms of functional units and native instructions.
Functional units are parts of the machine, such as logical compute qubits and
T-state factories. Native instructions are the operations they can execute.

Event handlers describe how the units behave: how they initialize, schedule
instructions and react when operations start or finish. For example, finishing a
factory round can make a T state available for a waiting injection. The model
computes resource estimates from its state. The temporal profiler handles
scheduling and advances the simulated clock.

Currently models must be implemented in Rust, using
[QpuModel](src/models.rs) inside this crate.

TODO: make it possible to compose a model from predefined functional units using
a Python API, without writing a new Rust model.

### DemoModel1

[DemoModel1](src/models/demo_model1.rs) is the only implemented temporal model.
It describes hypothetical hardware with two kinds of functional units: logical
compute qubits and T-state factories. It does not represent a specific real
quantum computer. Logical qubits use the surface code, and T gates consume states
produced by the factories.

The model lets us vary gate timings, factory count and code distance. It also
has a simple error model for logical memory and injected T states. The detailed
assumptions and formulas are in
[TemporalProfiler.md](src/profilers/TemporalProfiler.md#demomodel1).

## Python API

[QREProfiler](../qdk_package/qdk/qre/profiler.py) is a Python wrapper around a
concrete profiler. `QREProfiler(profiler).profile(application)` executes an
application using that profiler as its backend, then returns a `ProfilerResult`.

It supports these application types:

- `QSharpApplication`: a Q# expression or an imported callable with arguments.
- `OpenQASMApplication`: OpenQASM source or an imported callable with arguments.
- `QIRApplication`: QIR as LLVM text (`str`) or binary bitcode (`bytes`).
- `QspApplication`: native source code, compiled to an executable that streams
  instructions to the profiler over dedicated pipes. Program stdout remains
  available for its own output. See the
  [native application examples](../qspnlib/README.md).

For Q# and OpenQASM, the default path compiles the program to QIR, lowers it to
adaptive bytecode using PyQIR, and executes that bytecode. Pass `use_qir=False`
to use the Q# interpreter instead. QIR applications always use bytecode.
Aggregate `LogicalCounts` are not executable programs and cannot be profiled.

To profile a QIR file with configured instruction timings:

```python
from pathlib import Path

from qdk.qre import SequentialCountingProfiler
from qdk.qre.application import QIRApplication
from qdk.qre.profiler import QREProfiler

application = QIRApplication(Path("program.ll").read_text(encoding="utf-8"))
profiler = SequentialCountingProfiler(
    physical_qubits_per_logical=10,
    gate_time_sec={"h": 5e-6, "cx": 5e-6, "mz": 1e-6, "reset": 1e-6},
)
result = QREProfiler(profiler).profile(application)
print(result.totals.total_runtime_sec)
result.to_json("profile.json")
```

`QREProfiler.counting()` creates a wrapper configured to count every QSP
instruction, with zero gate durations and one physical qubit per logical qubit.

For temporal profiling, construct `TemporalProfiler(model, config)`.
`DemoModel1` describes the hardware; `TemporalProfilerConfig` controls recording
and randomness. Pass the profiler to the same `QREProfiler` wrapper.
See the [construction example](src/profilers/TemporalProfiler.md#construction-and-adapters)
for the model parameters and required adapters.

The result exposes:

- `totals`: logical and physical qubit usage, runtime, operation counts,
  per-operation maps and the modeled error budget.
- `profiles`: time series collected during execution.
- `infos`: informational messages, such as missing gate timings.

`result.to_json(path)` saves the full result, including a trace when recording
is enabled. `ProfilerResult.from_json(json_text)` loads it again. The
[profile viewer](profile_viewer/README.md) can display these JSON files.

The Python `Profiler` base class also exposes lower-level methods:
`profile_qsharp_expr()`, `profile_qsharp()`, `profile_adaptive_program()`,
`profile_stdio()` and, on Unix, `profile_fds()`. The bytecode method takes the
dictionary produced by the adaptive lowering pass, not raw QIR.
Profiler instances retain state across calls. Create a new instance for an
independent run; failures do not roll back work already processed.
