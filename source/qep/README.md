## Universal interface between quantum layers of abstractions

We define a universal interface between 2 components of a quantum system.

### Driver and backend.
* These 2 components are called "Driver" and "Backend".
* Driver sends instructions to Backend.
* Backend receives instructions, executes them and sends results to Driver.
* These names are only in context of this interface, the same component can be Driver
    relative to one component, and backend for other.
* Examples:
    * The adaptive bytecode runner is a Driver which performs classical instructions
        and sends quantum instructions to Backend.
    * A simulator can implement Backend.
    * A profiler can implement Backend. Profiler is similar to simulator, but it doesn't
       maintain wavefunction. Instead, it tracks calls and after execution reports
       resource estimates for program (runtime, physical qubits).
    * Qodec is a Driver relative to some Backend (which can be real quantum computer, profiler or simulator).
    * Qodec is a Backend relative to the bytecode runner.
    * Qodec can be composed of multiple qodecs connected via Driver-Backend interface.
    * Real quantum computer can implement Backend.
    * We might insert networking layer in place of Backend-Driver interface. For example:
       * We have a client on local machine. It implements Backend interface. 
       * We have a bytecode runner running on the local machine, using the client as its Backend.
       * We have a real quantum engine running on remote machine, implementing Backend Interface.
       * We have a server running on remote machine, using real quantum engine as its Backend.
       * Server and client talk to each other over HTTP.
        
### Instruction batches

The driver passes ordered slices of instructions to the backend. Batching avoids the
per-instruction synchronization and allocation costs of communicating through channels.
The runners accumulate instructions until they reach either:

* A result-producing instruction (`read_result`, `mz`, or `mz_reset`), which is
  always the last instruction in its batch.
* The configured maximum batch size. The default is 10,000 instructions.

Backends execute instructions in order and return exactly one result for each
`read_result`, `mz`, or `mz_reset` instruction, in the same order. Reads can occur anywhere in a
batch, and a batch can contain multiple reads.

The runners submit batches with at most one result-producing instruction, always
as the final instruction. They validate backend responses as follows:
* A batch ending in a result-producing instruction must return exactly one `QuantumBackendResult::Zero`
  or `QuantumBackendResult::One`.
* A batch without `read_result` must return an empty result list.
* A `QuantumBackendResult::Error` fails execution; unexpected or extra results are rejected.

`BatchResult.results` is an ordered `SmallVec<[QuantumBackendResult; 1]>`. It supports
arbitrary-length result lists, but keeps the runners' zero-or-one-result case
inline without heap allocation. Larger lists allocate only when the inline capacity
is exceeded. `smallvec` avoids requiring a default result value, unlike `tinyvec`.
Backends can use `QuantumBackendResult::from(bool)` (or `.into()`) to convert
measurement bits before pushing them into `BatchResult.results`.

### Quantum Backend trait

Backend must implement the following trait:

```rust
trait QuantumBackend {
    fn execute_batch(&mut self, instructions: &[QuantumBackendInstruction]) -> Result<BatchResult, String>;
}
```

`execute_batch` returns the requested results synchronously. The temporal profiler may
leave result-free instructions pending, while preserving their dependency order.
The driver reuses its instruction buffer across calls.
Backend failures are returned as `Err(String)` and propagated by adapters and the runners.

`QuantumBackends::sparse_sim()` creates a sparse simulator wrapped in
`QsBackendShim`, supporting all QEP instructions. Qubits are allocated lazily;
stored QIR result IDs are maintained separately across batches, so immediate
measurements cannot overwrite them. Reading an unwritten result or measuring a
lost qubit returns an error. The underlying simulator is available through
`backend()` and `backend_mut()`.

`QftRotationSynthesisAdapter` replaces `rz` angles of +/- 2^k * pi for
k >= -29 with Clifford+T sequences (including X and H gates). It accepts absolute
angle discrepancies up to `1e-10` radians, including angles near zero.
The decomposition is up to global phase. For k >= -2, the emitted phase gate is
`exp(i * angle / 2) * Rz(angle)`; smaller angles use approximate sequences.
The synthesis notebook `tmp/Synthesis.ipynb` writes `src/adapters/qft_rotations.inc`,
an array of strings parsed by Rust's `include!` at compile time. Normal builds
require neither Python nor `pygridsynth`. Run the notebook from within the
repository using an environment with `mpmath` and `pygridsynth` to regenerate
the table at synthesis precision `1e-10`.
Non-finite angles and other rotation types return errors; arbitrary-angle
synthesis is not yet implemented. Non-rotation instructions and backend results
pass through unchanged.

`QuantumBackendInstruction` directly stores the instruction name and a unified
operand list in `classical_parameters`. The bytecode runner emits it directly, and
backends dispatch on literal `instruction_name` strings and operand slices.
Names are `&'static str`; the operand list uses `SmallVec<[Parameter; 3]>`.
Every supported QIR instruction fits inline, without per-instruction heap allocation.

Qubit operands are `Parameter::QubitId` values and always precede other parameters.
Rotations append a `Parameter::Float` angle; `mz_store` and `mz_store_reset`
append a `Parameter::ResultId`. A `read_result` has only a `Parameter::ResultId`
and no qubit operands. For example, `rx` takes `[QubitId(q), Float(angle)]`, while
`mz_store` takes `[QubitId(q), ResultId(r)]`. Qubit iteration filters the operand
list for `Parameter::QubitId`, ignoring angles and result IDs.
The `instructions` instruction table
defines canonical IDs (such as `H` and `READ_RESULT`), lowercase names, and operand
shapes without aliases. `instruction_id_to_name` and `instruction_name_to_id` convert
between IDs and names. Measurement instructions are `MZ_STORE` (21, `mz_store`) and
`MZ_STORE_RESET` (22, `mz_store_reset`). Unknown IDs panic; unknown names return `None`.
Immediate measurements are `MZ` (24, `mz`) and `MZ_RESET` (25, `mz_reset`).
Both take a single `Parameter::QubitId`. They append the measured
bit directly to `BatchResult.results` without writing a result slot; `mz_reset` also
resets the qubit to zero after measuring. Results from `mz`, `mz_reset`, and
`read_result` appear in instruction order. Store variants do not append results.
The profiler constructor converts unknown names into a string error. `validate_instruction` borrows
a `QuantumBackendInstruction` and panics for an unknown name or invalid operand shape,
including incorrect parameter types or ordering.
Validation can be performed explicitly before execution; the counting profiler
does not repeat operand validation in its hot loop. Its constructor returns
`Result<Self, String>` and rejects unknown timing names and negative or non-finite
durations. The profiler resolves names to internal `usize` instruction IDs.
Counters are a zero-initialized array indexed by ID, sized to the
highest configured timing ID plus one (empty for an empty timing map).
Executed instructions within that range retain call counts even when their timing
is missing; they contribute zero runtime and a missing-timing warning. IDs beyond
the range are recorded in a set and warned about once, without retaining call
counts or per-operation runtime entries. Unknown instruction names are also
reported once without call counts. Configure timings for all executed
instructions to keep counting on the array-index path after name resolution.
`QuantumBackendInstruction` is the sole instruction representation used by the runner,
backends, profiler, and tests.

## Adaptive bytecode runner

`run_adaptive_program(&program, &mut backend)` executes one already-lowered
`qdk_simulators::bytecode::AdaptiveProgram<u64>` using the Rust `run_shot`
interpreter. It does not parse `.ll` or `.bc` files. Programs must satisfy the
interpreter's well-formed bytecode requirements.

The runner adapts `QuantumBackend` to the simulator trait and uses `Batcher`.
Gates flush at the default capacity of 10,000 instructions or on successful
completion. Measurements use immediate `mz`/`mz_reset` barriers and cache their
values in local result slots for classical control flow. Classical result writes
update only those local slots. Backend state is not reset between invocations.

The return type is `Result<RunOutput, String>`. Records contain leaf result,
boolean, integer, and double values; labels and `return_value` are `None`.
Array/tuple markers are omitted by the bytecode interpreter. Programs without
output-recording instructions produce no output records.

CCX is forwarded as a single `ccx` instruction with its two controls followed
by its target. Decomposition is left to the backend's transpiler adapter.
SX-adjoint is decomposed into H, S-adjoint, H. Movement, loss inspection, and
explicit noise operations return errors because the QEP instruction set cannot
represent them. Backend failures and invalid backend responses are returned as
errors as well. Since simulator methods cannot return errors, the first adapter
error is retained until interpretation finishes; subsequent backend calls are
suppressed and pending instructions are discarded. Already-executed backend
changes are not rolled back.

## Q# expression runner

`run_qsharp_expr` executes an expression in an existing Q# interpreter and forwards
quantum instructions to a backend. `run_qsharp_call` invokes an already-resolved callable.
Both discard classical output and return values, and flush buffered gates on success.

Each measurement emits `mz_store` or `mz_store_reset`, followed immediately by
`read_result`, so the interpreter receives a concrete result before continuing.
`ResultAdapter` can translate these result-register operations for backends that
only support immediate measurements.

The native QIR file runner has been removed. QIR files can be parsed and lowered
to adaptive bytecode in Python with PyQIR; the Rust QEP crate does not require
LLVM development libraries.

## Quantum Execution Profiler

A Profiler is a Backend that models a specific QPU (Quantum processing unit/quantum engine).

`Profiler::profile_adaptive_program(&program)` executes one shot of already-lowered
64-bit adaptive bytecode and returns the accumulated profiling data, discarding
program output. It uses the adaptive bytecode runner described above.

The Python `Profiler` base class exposes the same method to both counting and
temporal profilers. Pass the dictionary emitted by the existing Python lowering
pass, rather than a PyQIR module or raw LLVM bitcode:

```python
import pyqir
from qdk._adaptive_pass import AdaptiveProfilePass, Bytecode
from qdk.qre import SequentialCountingProfiler

module = pyqir.Module.from_ir(pyqir.Context(), qir_text)
program = AdaptiveProfilePass(Bytecode.Bit64).run(module)
profiler = SequentialCountingProfiler(10, {"x": 0.25, "mz": 0.5})
result = profiler.profile_adaptive_program(program.as_dict())
```

Only trusted, well-formed bytecode is supported. Dictionary conversion errors
retain their Python exception types, and execution failures raise `RuntimeError`.
Repeated calls retain earlier profiling data; failures do not roll back
instructions that have already reached the backend.

Profiler can be implemented as:
 * Rust struct implementing `trait QuantumBackend`.
 * A library with any language that implements QIR ABI (or other ISA for physical profilers)
     that is then statically linked with a driver, which will require a shim. That shim will be a rust implementation
     of `trait Backend` that implements instructions as function calls.
     (TODO: implement this shim and then just refer to it here)
 * A web server that implements QIR ABI (or other ISA for physical profilers) as HTTP endpoints
   (or one endpoint where instruction name and args are in request body). This again requires a shim
   similar to previous item. This shim will be a rust implementation of `trait Backend` which will be a client
   to that server that will implement instructions by sending web requests.

Profiler can be logical or physical:
 * Logical profiler that implements QIR ABI (or other logical ISA). This profiler will rely on hardcoded
    information on:
    * how long each gate takes
    * how many physical qubits are needed to encode each logical qubit
    * How many physical qubits are needed for factories
 * Physical profiler will implement ISA of specific real quantum hardware, which is typically very limited
   (e.g. CZ, MZ and few single-qubit gates for Atom Computing). It must be used together with a Qodec. 

### Implementing the Profiler

Profiler can be written:
 * By a human expert with knowledge about specific quantum computer who understand it physics etc.
   * In theory, we can ask vendors to supply such a profiler for each machine model.
 * Or by an agent, given a paper with detailed enough description of specific hardware.
   * For good results, this agent will also require good documentation of the framework, 
       guidelines for implementing profilers and several high quality examples.


### Implementing measurements
We cannot correctly predict probability of measurement outcomes without keeping track of 
wavefunction which takes in general O(2^n) memory. We can do this only for very small 
number of qubits, and resource estimation is typically done when number of qubits is so large
that simulation is impossible.

So, we suggest implementing a simple heuristic:
* Every qubit is in state ZERO, ONE or ENTANGLED.
* Initially or after reset it is in state ZERO.
* Applying X moves it to ONE.
* Applying any gate moves all qubits it applies to, to ENTANGLED.
* Measurement on ZERO or ONE returns result equal to state and doesn't change qubit.
* Measurement in ENTANGLED state will with probability 50% move qubit to state ZERO and
    with probability 50% to state ONE, and move qubit to corresponding state (ZERO or ONE).

This strategy should be close to truth when 
* We only do measurements in the end.
* Only measurements inside circuit are for measurement-based uncompute.

There are cases when this strategy will significantly impact estimation.
* For example, if we have repeat-until-success loop where success is defined by 
measuring single qubit in state 0, and actual success rate is e.g. 1%. Our model will get
success rate 50% and underestimate time by factor of 50x.
* Such cases should not be estimated with this profiler or we should use different strategies.


There may be other ways to do it:
* Always return 0 or 1 w.p. 50% from read_result, don't do any measurements at all.
* Do Clifford simulation and give up when encountering non-Clifford gate.
* Maintain connected components and simulate state only within connected components, and
    replace cross-component gates with classical approximations (in style of Semiclassical QFT).
* Annotate measurement with expected probability of outcome |1>. This probability can be sent to backend
    by instruction "set_probability_of_next_measurement".

Anyway, all these are implementation details of the Profiler. While we will provide some
reusable modules to implement "reduced simulation" in Profilers, implementers are free to do
it any way they want.



### Simple profiler: SequentialProfiler

We will implement a simple generic profiler called SequentialProfiler, based on following assumptions:
* All instructions are executed sequentially (no parallelism).
* Each instruction takes fixed time which only depends on instruction types.
* Physical qubit usage is equal to logical qubits usage times some constant.

In implementation this profiler will maintain call counter for every instruction.
In the end total runtime will be computed as `sum (gate_time[i]*gate_calls[i])`.

It's not doing much more than logical counter (`counts.rs`), but it's a starting point 
and a proof-of-concept for the whole QEP project.


### Advanced profiler: TemporalProfiler

`TemporalProfiler<M>` is an event-driven profiler that separates operation scheduling
from the quantum hardware model. The generic scheduler maintains the simulated clock,
per-functional-unit dependencies, resource availability, completion events, and batch results.
Independent operations can run concurrently once their dependencies and resource requirements
are satisfied.

The [`QpuModel`](src/models.rs) implementation defines native operations, resources, and functional units.
Hardware models live under [`src/models/`](src/models/), with
[`DemoModel1`](src/models/demo_model1.rs) using the factory and logical-qubit models there.
The model callbacks are:
* `on_start` schedules initial work, such as factory distillation rounds.
* `on_instruction_received` converts a backend instruction into native operations and schedules
  them with their targets, resource requirements, durations, and result requirements.
* `on_operation_started` assigns available resources and updates the operation before logging.
* `on_operation_finished` produces results, publishes resources, and schedules follow-up work.
* `get_total_counts` computes model-specific resource estimates from the current profiler state.

`TemporalProfilerState::schedule_operation` automatically orders operations sharing a target.
Resources published through `add_resource` wake waiting operations. Factory rounds use the
same scheduling machinery as gate operations. `EventRecorder` collects profiles and optional
traces; native operations implement `LoggableOperation` to provide their metric names and
trace representation.

#### Demo hardware model

`TemporalProfiler::new(model, config)` constructs a `TemporalProfiler<DemoModel1>` from
the hardware model and a separate `TemporalProfilerConfig`. This model has the following behavior:
* Native instructions are `cx`, `h`, `s`, `sadj`, `x`, `y`, `z`, `t`, `tadj`, `mz`, and `reset`.
* Instructions are ordered by their per-qubit dependencies. Independent
  instructions can run concurrently, including successors unblocked by the same operation.
* Instruction completion events are ordered by timestamp, with equal-time events processed FIFO.
* Rust's default model uses distance 9 and a conservative uniform Clifford latency of
  three logical time steps (`3 * ecc_distance * 1,000` ns, or 27,000 ns).
  Python's `DemoModel1` accepts an explicit `clifford_gate_time_nanos`; the demo uses 5,000 ns.
  Measurement and reset each use `measurment_time_nanos` (1,000 ns by default).
  Explicit timing overrides are not automatically rescaled when distance changes.
* Every T factory starts an `11 * ecc_distance * 1,000` ns distillation round at startup. Each attempt succeeds with
  probability `1 - 15 * p_T - 356 * epsilon(d)`; failures immediately retry.
  A successful factory holds one T-state and pauses until that state is consumed.
  The compact single-round 15-to-1 factory uses five logical patches and 11 logical steps,
  following [Litinski, arXiv:1808.02892](https://arxiv.org/abs/1808.02892).
  Output error remains `35 * p_T^3 + 7.1 * epsilon(d)`; its fidelity and acceptance
  coefficients from [arXiv:2211.07629, Appendix C / Table 6](https://arxiv.org/abs/2211.07629)
  are retained as demo approximations, not a calibration of the compact layout.
* `t` and `tadj` reserve a ready factory and execute as `inject_t` and `inject_tadj`, with operands
  `[FactoryId, QubitId]`. Injection takes twice the Clifford duration plus the measurement
  duration (55,000 ns by default). The factory starts its next round only when injection finishes.
  Ready factories are selected FIFO within each resource type.
* Instruction references and qubit dependencies persist across QIR batches, including the
  adaptive-profile result barrier. `ResultAdapter` maintains result registers across batches.
* `DemoModel1` configures gate timings, factory count, and ECC distance, defaulting to ten factories and distance nine.
  `TemporalProfilerConfig` contains only `aggregation_window_nanos`, `seed`, and `record_trace`.
  Defaults use one-second windows, no seed, and no trace recording. A fixed seed reproduces both distillation
  outcomes and measurement bits. Construction requires positive distance, at least one factory, and a nonzero window.

The demo explicitly assumes a future hardware target with physical Clifford and raw T-state
errors of `1e-4`, a threshold of `0.01`, and a 1,000 ns physical syndrome cycle.
[Litinski, arXiv:1808.02892](https://arxiv.org/abs/1808.02892) explores both `1e-3` and
`1e-4` physical errors with 1 microsecond cycles. These are forecasting assumptions, not a
calibration of current hardware. Compute memory and factories share
`epsilon(d) = 0.03 * (1e-4 / 0.01)^((d + 1) / 2)` per logical time step, following the
convention of [arXiv:2211.07629, Appendix B / Table 5](https://arxiv.org/abs/2211.07629).
The memory budget per nanosecond is `epsilon(d) / (d * 1,000)`; do not mix this convention
with fits quoted per physical syndrome cycle.

`TotalCounts.error` is a first-order additive fault budget, not an exact probability:
it can exceed one. Measurement charges only previously unreported memory and injected-state
contributions; it does not physically correct a noisy state. Reset starts a fresh interval,
and qubits are not charged for time before their first submission. Produced T states accrue
memory error while waiting and during injection. Resets are assumed perfect, and the model
does not simulate error propagation, routing, decoder delays, synthesis error, or intrinsic
algorithm success. The Shor demo asserts a modeled budget below 0.33 for its 16- and 50-bit
single-shot workloads, not for RSA-2048.

#### Construction and adapters

For QIR and non-native gates, wrap the Rust profiler in this order:

```rust
ResultAdapter::new(TranspilingAdapter::with_defaults(make_profiler_for_demo_model(config)))
```

`ResultAdapter` maps `mz_store` to `mz`, `mz_store_reset` to `mz_reset`, and handles
`read_result` without scheduling a quantum operation. The inner transpiling adapter decomposes
`mz_reset` into `mz` followed by `reset` (2,000 ns total by default). It also decomposes CY, CZ,
SX, SWAP, Toffoli, and supported QFT rotations into native gates. Profiles and traces report the
decomposed operations, not result-register accesses or the original composite gates.

The C temporal-profiler entry point installs these adapters automatically.
Python takes the hardware model and profiler configuration separately and installs adapters explicitly.
`DemoModel1` requires both timing arguments:

```python
from qdk.qre import DemoModel1, TemporalProfilerConfig, TemporalProfiler

model = DemoModel1(
    clifford_gate_time_nanos=27_000,
    measurment_time_nanos=1_000,
    ecc_distance=9,
)
profiler = TemporalProfiler(model, TemporalProfilerConfig())
profiler.add_adapter("transpiler")
profiler.add_adapter("result_adapter")
```

Changing instruction timings does not change the separate factory distillation duration.

#### Results and execution boundaries

`TemporalProfiler.get_result()` returns logical qubit usage, simulated elapsed time, operation
counts, rate profiles, and an execution trace when enabled. Operations are logged when they
start, with their scheduled completion times. Round starts count as `round` operations;
injections count as `inject_t`/`inject_tadj`, not `t`/`tadj`. The trace includes all scheduled round
intervals, even rounds extending beyond execution. Pending rounds do not extend the reported
runtime: each batch stops as soon as all its result-producing instructions finish.
Independent results can complete out of submission order; all requested results are returned
in submission order. Batches without results, including empty batches, do not advance the
event loop. Pending instructions retain their dependencies across batches.

No final drain is performed: trailing result-free work may remain pending even when
program execution returns. Profiling results describe work processed so far, not necessarily
the complete submitted program.

`Factory utilization` sums distillation-round durations in nanoseconds and divides by the
aggregation window size in nanoseconds, reporting the average number of factories in use
(units: `factories`) without percentage scaling. The viewer displays these
values and units directly. Waiting time is excluded. Each round is charged to its starting
window rather than split across window boundaries.
Event rates and factory utilization use separate `TumblingWindowAggregator` instances,
configured with `AggregationType::Rate` and `AggregationType::Utilization`, respectively.
Only the event aggregator contributes to the total operation/event count; their time series
are combined in the result.


### Resource estimation workflow.

On a high level, resource estimation looks like this:
* A Q# or OpenQASM application is compiled into QIR.
* We instantiate a profiler for the target quantum computer.
* In Python, PyQIR parses QIR and lowers it to adaptive bytecode, which runs with the profiler as backend.
* `QREProfiler(profiler).profile(application)` returns `ProfilerResult` with qubit counts, total runtime, operation counts,
    and runtime per operation.

Rust profilers can execute Q# directly with `Profiler::profile_qsharp_expr`, given an interpreter
and an entry expression, or execute lowered bytecode with `Profiler::profile_adaptive_program`.

### Python API

For Python users we expose:
* `QSharpApplication` and `OpenQASMApplication`, which wrap source code or imported callables with arguments.
* `QIRApplication`, which wraps QIR as LLVM IR text (`str`) or binary bitcode (`bytes`).
* `QREProfiler(profiler).profile(application)`, which compiles to QIR and profiles adaptive bytecode in memory by default.
  Pass `use_qir=False` to profile Q# or OpenQASM through the interpreter instead; QIR applications always use bytecode.
  Aggregate `LogicalCounts` are not executable and cannot be profiled.
* `QREProfiler(profiler).profile_qir_file(qir_file)`, which profiles a QIR file through PyQIR and the bytecode runner.
  Import `QREProfiler` from `qdk.qre.profiler`; the Python `Profiler` class no longer exposes `profile_qir`.
* `Profiler`, the base class for profiler implementations.
* `SequentialCountingProfiler`, one implementation of `Profiler`.
* `ProfilerResult`, which contains all profiling results.

For example, profiling with `SequentialCountingProfiler` looks like:
```python
from qdk import qsharp
from qdk.qre import SequentialCountingProfiler
from qdk.qre.application import QSharpApplication
from qdk.qre.profiler import QREProfiler

qsharp.init(target_profile=qsharp.TargetProfile.Base)
application = QSharpApplication("{ use q = Qubit(); X(q); }")
profiler = SequentialCountingProfiler(10, {"x": 2.5e-9})
result = QREProfiler(profiler).profile(application)
print(result.totals.total_runtime_sec)
```
