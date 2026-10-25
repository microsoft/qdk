# Shared Simulator Execution

This directory separates Adaptive control from target-specific quantum-state
evolution. The public API is `qdk_simulators::execution`; `execution.rs` is the
facade and the files here own the implementation. It is built on the executor
protocol (`executor_protocol`): the walk is a Runtime adapter, an executor of
programs that runs their classical part itself and sends the rest, in the
region ISA, to an executor of regions, measurements and resets (a terminal).
This README details its contracts.

## Module Responsibilities

| File                             | Responsibility                                                                                               |
| -------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| `adaptive.rs`                    | Prepares Adaptive bytecode, identifies unitary regions, interprets classical control, and produces commands. |
| `protocol.rs`                    | Defines the walk's commands and responses, and the region ISA (`RegionInstruction`).                         |
| `region.rs`                      | Defines target-neutral quantum evolution regions.                                                            |
| `unitary.rs`                     | Defines resolved unitary operations and the legacy `Simulator` application bridge.                           |
| `runtime.rs`                     | Runs the walk as a Runtime adapter (`RuntimeAdapter`) over any executor of the region ISA.                   |
| `terminal.rs`                    | Runs the region ISA on a legacy `Simulator` (`SimulatorTerminal`), an L3 terminal.                           |
| `operator_matrix.rs`             | Defines each gate matrix and the basis operators \|r⟩⟨b\| once, for every backend.                           |
| `pauli_sum.rs`                   | Defines backend-neutral Pauli-sum observables, which each backend evaluates on its own state.                |
| `tooling/conditioned_circuit.rs` | Builds a program conditioned on given measurement outcomes, as one circuit, to check simulation methods.     |

The source-level dependency direction is:

```text
Adaptive bytecode
      |
      v
 adaptive.rs -------> protocol.rs
      |                    |
      v                    v
 runtime.rs           region.rs
      |                    |
      | region ISA         v
      v                unitary.rs
 terminal.rs, or any       |
 Executor<Batch<RegionInstruction>>
```

The bytecode is a control-plan representation. A terminal does not interpret
bytecode or select branches. It receives only reached regions and host-visible
requests.

The other files sit beside that path. `operator_matrix.rs` depends only on
`unitary.rs`, and `pauli_sum.rs` on nothing in this directory; backends use
both directly. `tooling/conditioned_circuit.rs` is a terminal like any other:
it runs one shot through `RuntimeAdapter::run`.

## Glossary

- **Region**: a `QuantumEvolutionRegion`, a maximal run of consecutive unitary
  operations in a basic block, cut once when the program is prepared.
- **Terminal**: an `Executor<Batch<RegionInstruction>>`, which evolves one
  shot's quantum state; the target in the diagrams below.
- **Command** and **response**: what `AdaptiveExecution` asks for
  (`AdaptiveCommand`) and what the Runtime adapter answers (`AdaptiveResponse`).
- **Legacy runtime**: `bytecode::runtime::run_shot`, which interprets bytecode
  and calls a `Simulator` one operation at a time. It is the production CPU and
  Clifford path, and the parity reference for this directory.
- **Method** and **backend**: as in the design, a method is how the state is
  represented (MPS) and a backend is the library that runs it (cuTensorNet).
  The execution core depends on neither.

## Infrastructure Sharing vs. Profile Semantics

Sharing `PreparedAdaptiveProgram` and `AdaptiveExecution` across Base and
Adaptive QIR simplifies the implementation; it does not merge their QIR-level
contracts. Base Profile's no-branching, statically resolvable guarantee is
enforced independently by QIR validation, regardless of which backend executes
the program.

`QuantumEvolutionRegion` enables that sharing without a profile-specific
execution mode. Base Profile is the one-region restriction of the same general
structure rather than a separate control implementation.

## Execution Flow

`PreparedAdaptiveProgram` retains the original bytecode control tables and
caches deterministic region locations once. Each `AdaptiveExecution` owns the
mutable state for one shot: its instruction position, registers, measurement
results, ordered output records, and command/response protocol state.

```text
Python AdaptiveProfilePass
           |
           v
   AdaptiveProgram<Word>
           |
           v  prepare once per request
 PreparedAdaptiveProgram
           |
           +-----------------------+
           |                       | one per shot
           v                       v
   AdaptiveExecution         AdaptiveExecution ...
           |
           | AdaptiveCommand
           v
     RuntimeAdapter
           |
           | Batch<RegionInstruction>
           v
        terminal ------------> continuing target state
           |
           | Outcome per measurement
           v
     RuntimeAdapter -- AdaptiveResponse --> AdaptiveExecution
```

The command/response protocol is deliberately small:

```text
AdaptiveExecution::new
           |
           v
         Ready
           |
           +-- ExecuteRegion ---------> AwaitingRegionCompletion
           |                                      |
           |<---------- RegionComplete -----------+
           |
           +-- Measure --------------> AwaitingMeasurementResult
           |                                      |
           |<-------- Measurement(result) --------+
           |
           +-- Reset ----------------> AwaitingResetCompletion
           |                                      |
           |<----------- ResetComplete -----------+
           |
           +-- Complete(records) -----> Complete
```

`Reset` carries only its qubit and returns no value, but it is not unitary, so
it ends a region like a measurement does. Reset is an instruction of the region
ISA, so every terminal takes it: `SimulatorTerminal` calls `resetz`, and a
terminal that cannot represent reset fails rather than ignoring it.

A `QuantumEvolutionRegion` is uninterrupted target-local state evolution
between host-visible semantic boundaries. The current payload contains only
resolved unitary operations. Measurements, host-visible decisions and queries,
ordered output, and classical branch selection remain outside a region.
Regions carry no noise yet; in M2 the core draws it per shot.

`RuntimeAdapter` runs the walk over any terminal. It answers `RegionComplete`
and `ResetComplete` itself, and sends the terminal one batch per measurement:
the commands up to and including the next `Measure`, or up to the end of the
program, since the walk needs nothing else from the terminal before it goes on.
`RuntimeAdapter::run` gives a program's output records; as an
`Executor<Batch<PreparedAdaptiveProgram<u64>>>` it runs the programs of a call
in order on the same shot, and reports each record as an `Outcome` tagged with
its index:

```text
driver            RuntimeAdapter   AdaptiveExecution        terminal                 target
   |                     |                  |                     |                       |
   |-- call/run -------->|                  |                     |                       |
   |                     |-- next_command ->|                     |                       |
   |                     |<-- ExecuteRegion-|                     |                       |
   |                     |-- RegionComplete>|                     |                       |
   |                     |<-- Measure ------|                     |                       |
   |                     |-- call [ExecuteRegion, Measure] ------>|                       |
   |                     |                  |                     |-- apply operations -->|
   |                     |                  |                     |-- mz or mresetz ----->|
   |                     |<-- Outcome { tag: result_id, value } --|                       |
   |                     |-- Measurement -->|                     |                       |
   |                     |<-- Complete -----|                     |                       |
   |<-- records ---------|                  |                     |                       |
```

A terminal rejects a batch it cannot run (`Rejected`) before running any of it.
The Runtime adapter passes on a rejection only when the terminal ran nothing of
the call; afterwards the reply fails instead, since work had started.

The legacy `Simulator` (`FullStateSimulator` or `StabilizerSimulator`) never
receives a region and has no concept of one. `SimulatorTerminal` re-decomposes
each region into ordinary one-gate-at-a-time `Simulator` calls through
`apply_unitary_immediately` in `unitary.rs`, eagerly discarding the batching
opportunity. Region grouping is therefore a structural no-op for CPU and
Clifford, making parity with the legacy oracle direct. A tensor-network
terminal instead takes each region whole, which is what regions are for.

`SimulatorTerminal` is currently a compatibility implementation and parity
oracle. The legacy `bytecode::runtime::run_shot` remains the production CPU and
Clifford execution path until migration is explicitly validated.

## Shared Operator Table

```text
M[out][in], row-major: entry at out·d + in
two qubits (a, b): basis index 2a + b   (first operand most significant)
|r⟩⟨b| = e_r e_bᵀ:   |0⟩⟨1| = [[0,1],[0,0]] = [0, 1, 0, 0]
```

`unitary_matrix(UnitaryOperation)` and `basis_operator(r, b)` define every
gate value once (every `UnitaryOperation` except `I`, which backends drop).
The layout fixes orientation, not just entries: Y, Ry, CY and |0⟩⟨1| are not
symmetric, and their transposes would rotate the other way or select the
other branch. Backends consume it without re-deriving values. cuTensorNet,
for example, copies the row-major data verbatim, because
`cutensornetStateApplyTensorOperator` reads the textbook matrix correctly with
default (null) strides, operands in gate order; a backend with another layout
maps the entries itself. `basis_operator(r, b)` is the projection of one
measurement in a [conditioned circuit](#tooling-conditioned-circuits), applied
the same way as a non-unitary operator. `is_diagonal` tells a backend which gates it can bind by
their diagonal alone.

Each backend still decides which gates it accepts.

## Tooling: conditioned circuits

`execution::tooling` holds what tests and GPU qualification check simulation
methods against; no method depends on it. Its `ConditionedCircuit` is a
program conditioned on given measurement outcomes, as one circuit: evaluating
it without sampling gives the probability of those outcomes, a reference for
samplers.

[`ConditionedCircuit::from_prepared_program`](tooling/conditioned_circuit.rs)
runs one shot through the Runtime adapter, on a terminal that answers every
measurement from a given outcome record. Branches follow the record, so any number of regions yields one
linear circuit of unitaries and rank-one projections, which a backend can
evaluate without sampling. A reset folds into the preceding projection when the
qubit is in a known basis state; a reset after a gate is a channel with no
single-amplitude form and is rejected.
`ConditionedCircuit::with_outcome` changes one projection while keeping the
path, which checks that evaluation observes the projections independently of
control flow.

A record holds one value per result, so it describes a path only if every
result is measured at most once. A record that fails a selection check breaks
that: `qdk.stim.compile` lowers a Stim `SELECT` block to a loop that restarts
while any `REQUIRE` fails.

```text
Stim                     QIR from qdk.stim.compile
SELECT {                 select_0: reset q0, q1; H q1; CX q1, q0
  R 0 1                            mresetz q0 -> r0
  H 1                              restart = read_loss(r0) | read_result(r0)
  CX 1 0                           br restart ? select_0 : continue
  MR 0                   continue: mz q1 -> r1
  REQUIRE rec[-1]
}
M 1
```

| Record `[r0, r1]` | Selection check   | Result                                                    |
| ----------------- | ----------------- | --------------------------------------------------------- |
| `[0, 0]`          | passes            | `H q1; CX q1, q0; M q0 = 0 (reset); M q1 = 0`             |
| `[1, 1]`          | fails (`r0` is 1) | the loop measures `r0` again: `ResultMeasuredAgain { 0 }` |

This follows selection semantics, as in Stim-based samplers of these blocks: a
sampler discards records that fail a check and samples again, so `[1, 1]`,
although a possible outcome of one pass with probability 1/2, is never a
program output. Its probability among the outputs is 0.

`from_prepared_program` still reports an error rather than a zero probability,
for two reasons. No single circuit exists for such a record. And the same
detection fires when a program measures a result more than once on every path,
for example a loop reusing a result id, where zero would be the wrong answer
and conditioning is simply unsupported. A failing record usually means a wrong
bit order, a different program version, or bits taken from a failed attempt.

**TODO(selection-normalization): a conditioned circuit gives the probability
of one pass, not of a selected output.** Evaluating the circuit of a record
that passes every check gives $P_\text{pass}(r) = |\langle r|C(r)|0\ldots0\rangle|^2$.
A sampler repeats failed blocks, so its outputs follow $P_\text{pass}$ divided
by the acceptance probability. In the example above, `REQUIRE` passes with
probability 1/2 and samplers only output `[0, 0]`, whose probability as a
program output is therefore 1, while $P_\text{pass}([0, 0]) = 1/2$. The two agree
when every check passes with probability 1, as in noiseless error-correction
rounds where checks are stabilizer measurements. With noise, or any check that
can fail, a probability computed from this circuit is off by the acceptance
factor. Missing:

- computing the acceptance probability, a sum over rejected outcomes that needs
  a marginal (double-layer) network rather than one amplitude;
- fixing which normalization is meant: over the whole record (reject the whole
  shot) or per block given the state before it (retry the block, as the lowered
  loop does). They agree when the acceptance of each block does not depend on
  earlier outcomes and a failed attempt does not disturb qubits outside the
  block.

The restart resets qubits that are still in use before measuring `r0` again.
Those resets follow from the failed check, so they are reported as
`ResetOfLiveQubit` only if the program completes without measuring a result
again.

## Current Constraints

- The shared layer prepares generically over `Word` but currently executes only
  at `u64` (`AdaptiveExecution`, `RuntimeAdapter`); generalising the walk over
  `Word` is deferred. Terminals are word-agnostic by design — they receive `QuantumEvolutionRegion`s and never
  see bytecode — so the legacy GPU path's `Bytecode.Bit32` convention, which
  exists because that path executes bytecode on-device, does not apply to
  backend routes here.
- Shared execution detects output-recording instructions once during
  preparation. Without them, completion returns every result-register slot in
  index order, preserving default `Zero` values for unmeasured slots and
  `Loss` values returned by a terminal.
- Region preparation currently recognizes only the supported unitary operation
  subset.
- Adaptive control handles only the bytecode instructions exercised by the
  shared execution tests (`adaptive.rs` lists them). Any other opcode, such as
  `ICMP` (M1's `icmp`) or `OP_WRITE_RESULT`, is an `UnsupportedInstruction`
  error; the legacy runtime remains the complete oracle.
- The Runtime adapter runs a whole call before it returns, and serves one shot.
  Sessions of many shots, asynchronous terminals, and same-prefix replay
  require explicit designs rather than hidden terminal behavior.
- Existing backend entry points and result shapes must remain stable while a
  backend migrates to shared execution.
