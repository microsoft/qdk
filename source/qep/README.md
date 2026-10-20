## Universal interface between quantum layers of abstractions

We define a universal interface between 2 components of a quantum system.

### Driver and backend.
* These 2 components are called "Driver" and "Backend".
* Driver sends instructions to Backend.
* Backend receives instructions, executes them and sends results to Driver.
* These names are only in context of this interface, the same component can be Driver
    relative to one component, and backend for other.
* Examples:
    * QIR runner is a Driver which performs all classical instructions and sends 
        instructions from QIR ISA to Backend.
    * A simulator can implement Backend.
    * A profiler can implement Backend. Profiler is similar to simulator, but it doesn't
       maintain wavefunction. Instead, it tracks calls and after execution reports
       resource estimates for program (runtime, physical qubits).
    * Qodec is a Driver relative to some Backend (which can be real quantum computer, profiler or simulator).
    * Qodec is a Backend relative to QIR runner.
    * Qodec can be composed of multiple qodecs connected via Driver-Backend interface.
    * Real quantum computer can implement Backend.
    * We might insert networking layer in place of Backend-Driver interface. For example:
       * We have a client on local machine. It implements Backend interface. 
       * We have QIR runner running on local machine, using the client as its Backend.
       * We have a real quantum engine running on remote machine, implementing Backend Interface.
       * We have a server running on remote machine, using real quantum engine as its Backend.
       * Server and client talk to each other over HTTP.
        
### Instruction batches

The driver passes ordered slices of instructions to the backend. Batching avoids the
per-instruction synchronization and allocation costs of communicating through channels.
The QIR runner accumulates instructions until it reaches either:

* A `read_result` instruction, which is always the last instruction in its batch.
* The configured maximum batch size. The default is 10,000 instructions.

Backends execute instructions in order and return exactly one result for each
named `read_result` instruction, in the same order. Reads can occur anywhere in a
batch, and a batch can contain multiple reads.

The QIR runner continues to submit batches with at most one `read_result`, always
as the final instruction. It validates backend responses as follows:
* A batch ending in `read_result` must return exactly one `QuantumBackendResult::Zero`
  or `QuantumBackendResult::One`.
* A batch without `read_result` must return an empty result list.
* A `QuantumBackendResult::Error` fails execution; unexpected or extra results are rejected.

`BatchResult.results` is an ordered `SmallVec<[QuantumBackendResult; 1]>`. It supports
arbitrary-length result lists, but keeps the QIR runner's zero-or-one-result case
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

`execute_batch` processes all instructions synchronously and in order before returning.
The driver reuses its instruction buffer across calls.
Backend failures are returned as `Err(String)` and propagated by adapters and the QIR runner.

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
operand list in `classical_parameters`. The QIR runner emits it directly, and
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

## QIR Runner

QIR Runner is a Driver.
 * It reads QIR from file, and executes it. 
 * It owns a Backend. 
 * It forwards quantum instructions from QIR to Backend.

Measurements are done in somewhat weird way, according to QIR spec:
 * There is a "measurement array" and it must be maintained by Backend. 
 * When QIR Runner sees QIR measurements (`m` or `mz`, or `mresetz`), it enqueues
     the corresponding QEP operation (`mz_store` or `mz_store_reset`)
     and doesn't wait for it, as per QIR spec. Backend will perform measurement and write result to 
     specified index at measurement array.
 * When it sees QIR instruction that instructs it to read result from result register (read_result or *_record_output),
    it finishes the current batch with a Read command containing result_id and synchronously obtains
    the returned bit. Then it uses that value in classical flow.
 * It is responsibility of Backend to perform measurement that writes to result_id before responding to ReadResult.
 * There is __quantum__rt__write_result operation that technically means that we send classical bit from Driver to Backend
    and Backend must store it at specified index in results array. It exists only for Stim simulator.
    Our QIR runner doesn't implement it, it will fail during execution when encounters this command.
    So write_result is not included in the instruction table.

"QIR ABI" refers to specific set of instructions which roughly corresponds to library/std/src/Intrinsic.qs
and for its exact definition see the instruction table in `instructions.rs`.

## Quantum Execution Profiler

A Profiler is a Backend that models a specific QPU (Quantum processing unit/quantum engine).

Profiler can be implemented as:
 * Rust struct implementing `trait QuantumBackend`.
 * A library with any language that implements QIR ABI (or other ISA for physical profilers)
     that is then statically linked with QIR Runner, which will require a shim. That shim will be a rust implementation
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


### Advanced profiler: event queue based profiler.

`TemporalProfiler` is a prototype event-driven model with a simulated clock:
* Instructions are ordered by their per-qubit and measurement-result dependencies. Independent
  instructions can run concurrently, including successors unblocked by the same operation.
* Instruction completion events are ordered by timestamp, with equal-time events processed FIFO.
* The prototype assigns 1 ms to `h`, `x`, `t`, `cx`, and measurement-store instructions; `read_result`
  takes 0 ns. Other instructions are not yet supported.
* One T-state is produced every 1 ms, beginning at 1 ms. T gates consume one available T-state.
* Instruction references and qubit/result dependencies persist across QIR batches, including the
  adaptive-profile result barrier.

Temporal profile collection is not implemented yet. `TemporalProfiler.get_result()` currently
returns empty totals and profiles, and includes the simulated start/end timeline in `infos`.


### Resource estimation workflow.

On a high level, resource estimation looks like this:
* Application is compiled into QIR.
* We instantiate a profiler for the target quantum computer.
* We run QIR Runner with profiler as backend.
* `profile_qir` returns `ProfilerResult` with qubit counts, total runtime, operation counts,
    and runtime per operation.

### Python API

For Python users we expose:
* `QSharpApplication`, which is a wrapper around Q# code.
  * Its `profile` method writes QIR to a temporary file and removes it after profiling.
* `Profiler`, the base class for profiler implementations.
* `SequentialCountingProfiler`, one implementation of `Profiler`.
* `ProfilerResult`, which contains all profiling results.

For example, profiling with `SequentialCountingProfiler` looks like:
```python
from qdk import qsharp
from qdk.qre import SequentialCountingProfiler
from qdk.qre.application import QSharpApplication

qsharp.init(target_profile=qsharp.TargetProfile.Base)
application = QSharpApplication("{ use q = Qubit(); X(q); }")
profiler = SequentialCountingProfiler(10, {"x": 2.5e-9})
result = application.profile(profiler)
print(result.totals.total_runtime_sec)
```
