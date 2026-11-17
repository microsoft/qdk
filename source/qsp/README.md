# Quantum Service Protocol

Quantum Service Protocol (QSP) is a framework for sending quantum instructions
and results between components. A component executing a quantum program can use
different backends without changing the program. Adapters can transform
instructions and results between the components.

QSP defines:

- Core Rust interfaces.
- Instruction set.
- Serialization format.
- Interprocess communication protocol.

The package also provides utilities for working with QSP.

## Core Rust interfaces

We define a few core entities: Driver, Backend, Adapter and Runner.

Driver is something that sends quantum instructions to a downstream backend.
Instructions may be batched; sending one instruction is sending a batch of size 1.
Every time the driver sends a batch, it blocks until it receives the results.
Then it uses those results to continue execution, for example to choose a branch
based on a measurement.

The Rust [Driver](src/interface.rs) trait identifies the type of the downstream
backend and provides access to it through `get_backend()`.

Backend is something that can process batches of quantum instructions. It
can be a simulator, a model of a quantum computer, or a real quantum computer.
The [QuantumBackend](src/interface.rs) trait has one method, `execute_batch()`.
It receives an ordered slice of `QuantumBackendInstruction` and returns
`Result<BatchResult, String>`.

Each instruction contains an instruction ID and typed parameters:
`Parameter::QubitId`, `Parameter::ResultId` or `Parameter::Float`. A `BatchResult`
contains an ordered list of `QuantumBackendResult` values: `Zero`, `One` or
`Error`. There is exactly one result for every `mz`, `mz_reset` or `read_result`
instruction, in instruction order. Other instructions do not append results.
A backend can accept multiple result-producing instructions in one batch.

Adapter is something that can be plugged between a driver and a backend to
transform the instruction stream. It is a Driver for the downstream backend and
a Backend for the upstream driver. Adapters can be stateful and can transform
results as well as instructions. The
[Adapter](src/utils/adapters.rs) marker trait is implemented for types that
implement both `Driver` and `QuantumBackend`.

Runner is a driver that executes a quantum program. It performs classical
computation and sends quantum instructions to its backend. Runner is a term for
this role, not a separate Rust trait.

These names are relative to a connection between components. The same component
can be a backend for one component and a driver for another.

## Instruction set

The instruction set is part of QSP. It is defined in
[instructions.rs](src/instructions.rs), which gives each instruction a `u16` ID,
a canonical name and a parameter shape. Currently there are 27 instructions:

- Gates: `ccx`, `cx`, `cy`, `cz`, `rx`, `rxx`, `ry`, `ryy`, `rz`, `rzz`, `h`,
  `s`, `sadj`, `sx`, `t`, `tadj`, `x`, `y`, `z`, `swap`.
- Reset: `reset`.
- Measurement and result access: `mz_store`, `mz_store_reset`, `read_result`,
  `mz`, `mz_reset`.
- Adjoint AND: `and_adj`, a specialized three-qubit uncomputation operation.

We support two measurement models:

- Immediate measurement: `mz(qubit)` and `mz_reset(qubit)` return a result in
  the current batch response. `mz_reset` also resets the qubit.
- Deferred measurement: `mz_store(qubit,result)` and
  `mz_store_reset(qubit,result)` store the measurement in a backend result slot.
  The driver obtains it later using `read_result(result)`. This is the model
  used by QIR.

Qubit IDs and result IDs are distinct parameter types. Rotation angles follow
the qubit operands: for example, `rz` takes `[QubitId(q), Float(angle)]`, while
`mz_store` takes `[QubitId(q), ResultId(r)]`.

`instruction_id_to_name()` and `instruction_name_to_id()` convert between IDs and
names. `validate_instruction()` checks the ID and parameter shape, panicking for
an unsupported or malformed instruction. The instruction set is defined by the
package, not extended by sending arbitrary names at runtime.

## Serialization format

[serde.rs](src/serde.rs) implements the serialization format:

- A single instruction is represented as `name(arguments)`, for example
  `ccx(1,2,3)`.
- A batch is a JSON array of instruction strings, for example
  `["h(1)","ccx(1,2,3)","rz(3,-0.125)","mz_store(3,7)","read_result(7)"]`.
- A batch result is a quoted JSON string, for example `"0102"`. Each character
  represents one result: `0` for `Zero`, `1` for `One`, and `2` for `Error`.
- An empty batch is `[]`. An empty result list is `""`.

Names and operand order follow the instruction set. For example, the
controlled-controlled X instruction is named `ccx`, not `ccnot`.
Deserialization rejects unknown names, malformed operands and invalid result
characters.

The public functions are `serialize_batch()`, `deserialize_batch()`,
`serialize_result()` and `deserialize_result()`. In-process connections can pass
instructions and results directly without serialization.

## Interprocess communication protocol

Two components use this format to communicate over streams:

- [StreamBackend](src/serde/stream_backend.rs) sends instruction batches to a
  writer and reads results from a reader. To the local driver, it is a backend.
- [StreamDriver](src/serde/stream_driver.rs) reads instruction batches, executes
  them using an owned backend, and writes the results.

Each message is followed by a newline and flushed before waiting for a reply.
The protocol is synchronous: every batch receives one result message before the
next batch is sent, including batches with no measurement results.

The streams can be pipes, stdin/stdout or other streams implementing Rust's
`Read` and `Write` traits. `StreamBackend::stdio()` writes instructions to stdout
and reads results from stdin. `StreamDriver::stdio(backend)` does the opposite.
Use dedicated streams if the program also needs stdout for its own output.

`StreamDriver::process_next_batch()` serves one batch and returns `Ok(false)` on
clean instruction EOF. `run()` serves batches until EOF. Malformed messages,
truncated frames, I/O errors and backend execution failures return errors.
A backend execution failure has no separate wire encoding; the connection owner
must close the connection on failure to unblock the peer. An `Error` result
value is distinct from an execution failure.

## Utilities

The [utils](src/utils.rs) module contains implementations for executing programs
and connecting components. These utilities use QSP but are separate from the
protocol definition.

### Runners

The [runners](src/utils/runners.rs) module supports two ways to execute a program:

- Q# execution through the existing interpreter, using `run_qsharp_call()` or
  `run_qsharp_expr()`. A reverse shim converts the interpreter's backend calls
  to QSP instructions.
- Adaptive bytecode execution through `run_adaptive_program()`. It takes an
  already-lowered `AdaptiveProgram`, performs classical computation and sends
  quantum instructions to the supplied backend. It does not parse QIR itself.

Both runners use batching and wait for results when needed for classical
execution.

### Backends

[QsBackendShim](src/utils/backends/qs_backend_shim.rs) wraps an existing Q#
evaluator backend and makes it usable as a QSP backend. It maps qubit IDs,
maintains stored measurement results and dispatches QSP instructions to backend
methods.

[QuantumBackends](src/utils/backends.rs) provides constructors for a sparse
simulator (`sparse_sim()`) and a Clifford simulator (`clifford_sim(num_qubits)`).
The wrapped simulator is available through `backend()` and `backend_mut()`.

### Adapters and transpilers

[ResultAdapter](src/utils/adapters/result_adapter.rs) allows a driver using
deferred measurements to work with a backend supporting only immediate
measurements. It replaces stored measurements with immediate measurements,
maintains a local result register and answers subsequent result reads.

[TranspilingAdapter](src/utils/adapters/transpiling_adapter.rs) replaces
instructions using registered transpilers, selected by source instruction ID.
Unregistered instructions pass through unchanged. Generated instructions go
directly to the backend; they are not recursively passed through transpilers.

The [transpilers](src/utils/transpilers.rs) module contains:

- Clifford decompositions for `cy`, `cz` and `sx`.
- SWAP and Toffoli decompositions.
- Measurement-and-reset decomposition into separate measurement and reset.
- Arbitrary-angle Rz synthesis into Clifford+T, with optional caching.
- Rz synthesis for angles of the form plus or minus a power of two times pi,
  using a precomputed table.

`TranspilingAdapter::with_defaults()` installs the Clifford, SWAP, Toffoli,
measurement/reset and arbitrary-angle Rz transpilers. Use
`TranspilingAdapter::new()` to supply a custom set.

### Batching

[Batcher](src/utils/batcher.rs) accumulates instructions for a backend. It sends
a batch when it reaches the configured size, or when a result-producing
instruction requires a response. The default maximum batch size is 10,000.
Each result-producing instruction ends its batch.
Call `finish()` to submit any remaining instructions; dropping the batcher does
not flush them.
