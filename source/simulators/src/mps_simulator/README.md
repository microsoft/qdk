# The MPS simulator of QIR programs

The [`mps_simulator`](../mps_simulator.rs) module of `qdk_simulators` lets
the QDK simulate the QIR programs it compiles on matrix product states
(MPS), which hold many more qubits than a state vector when a program
entangles them little, as chemistry programs such as H₂'s phase
estimation (29 qubits) do. It runs on any engine of the
[`tensornet`](../../../tensornet/README.md) contract, for instance
`tensornet`'s exact reference, or cuTensorNet's on a GPU.

**The problem.** Running a QIR program on an MPS needs two things that do
not know each other: the program's classical part (branches on
mid-circuit measurements, loops, records), which the
[`qir`](../qir/README.md) module runs for every target as a machine that
hands each shot's quantum calls to a resumer; and a library that holds the
chains of tensors, which `tensornet`'s `MpsEngine` states in sites and
matrices only.

**How it is solved.** [`MpsResumer`](resumer.rs) is a `QirResumer` written
once over any `MpsEngine`: it runs each batch of quantum calls the machine
yields on an MPS the engine holds, and answers the measurements.
Everything that knows qubits is in this module:

- **the qubit order** ([`resumer/order.rs`](resumer/order.rs)): which site
  holds each qubit, chosen once per run from the program's two-qubit
  gates, so that qubits that interact sit close on the chain; no caller
  sees sites;
- **the gates as matrices** ([`resumer/lowering.rs`](resumer/lowering.rs))
  and **their fusion** into one- and two-site operators
  ([`fusion.rs`](fusion.rs));
- **the draws** ([`resumer/splitmix64.rs`](resumer/splitmix64.rs)): each
  shot draws its measurements from its own stream, seeded from the
  resumer's seed and the shot's id;
- **the capacity** ([`resumer.rs`](resumer.rs)): the max bond dimension
  $\chi$, 128 by default;
- **the report** ([`report.rs`](report.rs)): what a run cost, and a
  warning when truncation lost more than $10^{-3}$ of a shot's weight.

**Why this approach.**

- **One implementation for every engine.** Neither the machine nor the
  engine changes for this module, and a new engine (a new GPU library)
  runs the programs this module runs with no change here.
- **A run costs what its distinct histories cost.** Shots that drew the
  same values so far share one state, forked where their draws differ:
  one state for a program of no randomness, at most one per shot, and at
  most $\lfloor \log_2 N \rfloor + 1$ at once for $N$ shots, which
  bounds a GPU's memory. The header of [`resumer.rs`](resumer.rs) says
  how.
- **A shot records what it records alone,** whichever shots run beside
  it, as the `QirResumer` contract requires, since each shot draws from
  its own stream. That stream is the one the executor protocol (planned)
  will seed for the only participant of a chain that draws, so a chain of
  a driver and this method, its executor drawing as this method does,
  will reproduce the records.

**Evidence.** What checks each claim, and where it was verified:

- **Checked by every `cargo test`:** the module's tests, in
  [`src/tests/mps_simulator/`](../tests/mps_simulator/), on `tensornet`'s
  exact reference engine: the records, their independence from the shots
  beside them, the shared states and their bound, the failures, the
  fusion, the qubit order and the report.
- **Verified during development, by code not in this repository yet,**
  on the reference and on a CPU engine: the resumer passes the `qir`
  module's conformance kit for resumers (`qir::conformance::check`,
  proposed apart from this module); the noiseless programs of the
  `run_shot` Python tests
  ([`test_adaptive_cpu_bytecode.py`](../../../qdk_package/tests/test_adaptive_cpu_bytecode.py)
  and
  [`test_recursive_call_frames.py`](../../../qdk_package/tests/test_recursive_call_frames.py)),
  run through the resumer by a development harness, meet those tests'
  expectations, all but one, which calls a loss check and is refused; and
  H₂'s phase estimation runs on that CPU engine with 1,404 measurements
  and 10 resets a shot, largest bond 24, retained weight
  $1 - 7 \cdot 10^{-12}$, about 84 s a shot on one core, its readout's
  distribution within $10^{-9}$ of the exact one in total variation
  distance.
- **Expected, not yet verified with this resumer:** the cuTensorNet engine
  on a GPU. An earlier version of this method ran H₂ on an NVIDIA A100
  through cuTensorNet; [`fusion.rs`](fusion.rs) cites its timings.

## The model

- [`MpsResumer`](resumer.rs): the resumer, its settings
  (`with_max_bond_dimension`, `with_gate_fusion`) and the grouped run.
- [`GateFusion`](fusion.rs): `Pairs` (the default) or `Unfused`.
- [`MpsRunReport`, `TruncationWarning`](report.rs): the last run's
  largest bond and least retained weight, and the warning.
- [`MpsResumerError`](error.rs): why a shot failed, apart from a trap of
  its program.

A Bell pair, as the tests run it
([`src/tests/mps_simulator/resumer.rs`](../tests/mps_simulator/resumer.rs);
`program` writes the bytecode, and `expect_ready` polls a future that is
ready at once):

```rust
// The program: a Bell pair read into two results, in the bytecode the
// tests write.
let bell = program(
    2,
    &[&[gate1(OPID_H, 0), gate2(OPID_CX, 0, 1), mz(0), mz(1), ret()]],
);
// The resumer, on the exact reference engine, its draws seeded with 7.
let mut resumer = MpsResumer::new(ExactReference::new(), 7);
// One machine runs the program's classical part; the resumer runs the
// quantum calls of its 32 shots, and never waits.
let shots = QirShotId::first(32);
let records =
    expect_ready(resumer.run(&QirMachine::new(&bell), &shots)).expect("the program runs");
// In every shot, the two readouts agree.
assert!(records.iter().all(|shot| shot[0] == shot[1]));
```

## Planned

- **Noise and loss on matrix product states:** a shot that reaches a
  correlated noise, readout noise or loss check call fails, naming this
  item, and a noise model set on a simulator has no counterpart here. Its
  case: a logical Bell test on 56 qubits with Pauli noise and loss.
- **The sampled form:** a program that does not continue after a batch
  run once, its readout drawn for every shot with `MpsEngine::sample`. Its
  case: sampling a Base Profile program's many shots at the cost of one.
- **Queries** of the state (expectations of Pauli sums, the chain's cost).
  Its case: a chemistry user's energy estimate $\langle H \rangle$ read
  without shots.
- **The Python surface** that reaches this resumer. Its case:
  `run_qir(type="mps")` and `pip install qdk[tensornetwork]`.
- **Groups run in parallel** on CPU threads. Its case: the polyene of the
  chemistry case, whose 1,000 shots split into many histories.
