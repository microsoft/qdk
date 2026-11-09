# The MPS simulator of QIR programs

The [`mps_simulator`](../mps_simulator.rs) module of `qdk_simulators` runs
the QIR programs the QDK compiles on matrix product states (MPS), on any
engine of the [`tensornet`](../../../tensornet/README.md) contract: the
CPU's, cuTensorNet's on a GPU, or the exact reference.

## The problem

An MPS simulates many more qubits than a state vector when the program
entangles them little, as chemistry programs such as H₂'s phase
estimation (29 qubits) do. Running a QIR program on one needs
two things that do not know each other: the program's classical part
(branches on mid-circuit measurements, loops, records), which the
[`qir`](../qir/README.md) module runs for every target as a machine that hands each
shot's quantum calls to a resumer; and a library that holds the chains of
tensors, which `tensornet`'s `MpsEngine` states in sites and matrices only.

## The approach

`MpsResumer` is a `QirResumer` written once over any `MpsEngine`: it runs
each batch of quantum calls the machine yields on an MPS the engine holds,
and answers the measurements. Everything that knows qubits is here, so
neither the machine nor the engine changes for it, and a new engine (a new
GPU library) runs the programs this module runs with no change here:

- **the qubit order:** which site holds each qubit, chosen once per run by
  a local search over the program's two-qubit gates, so that qubits that
  interact sit close on the chain; it is applied inside the resumer, so no
  caller sees sites;
- **the gates as matrices** (`QirGate::matrix_at`, in the engine's layout)
  and **their fusion** into one- and two-site operators (`GateFusion`,
  pairs by default);
- **the draws:** each shot draws its measurements from its own stream,
  seeded from the resumer's seed and the shot's id as the executor
  protocol (planned) will seed the only participant of a chain that draws,
  so that a chain of a driver and this method, its executor drawing as
  this method does, will reproduce the records;
- **the capacity and the warning:** the max bond dimension χ (128 by
  default), and a warning when truncation lost more than 1e-3 of a shot's
  weight.

**Shots share states while their histories agree.** A run holds one MPS
per group of shots that drew the same values so far, and forks it where
their draws differ, so a run costs what its distinct histories cost: one
state for a program of no randomness, at most one per shot. Where a group
splits, the part with fewer shots goes on and the other waits, so a run of
N shots holds at most ⌊log₂ N⌋ + 1 states at once. Since each shot draws
from its own stream, a shot records what it records alone, whichever shots
run beside it, as the `QirResumer` contract requires.

The evidence is the module's tests, in
[`src/tests/mps_simulator/`](../tests/mps_simulator/), on the exact
reference engine. Beyond them, on a development branch: the resumer
passes the resumer conformance kit of the `qir` module and meets the
`run_shot` Python tests' expectations on their noiseless programs (all but
one, which calls a loss check and is refused), on the reference and on the
CPU engine; and H₂'s phase estimation runs on the CPU engine with the
qualified run's per-shot facts (1,404 measurements and 10 resets, largest
bond 24, retained weight 1 − 7·10⁻¹², about 84 s a shot), its readout's
distribution within 10⁻⁹ of the exact one in total variation distance.

## The model

- [`MpsResumer`](resumer.rs): the resumer, its settings
  (`with_max_bond_dimension`, `with_gate_fusion`, and `with_stop`, a flag
  that stops a run within one batch, for a user's interrupt) and the
  grouped run.
- [`GateFusion`](fusion.rs): `Pairs` (the default) or `Unfused`.
- [`MpsRunReport`, `TruncationWarning`](report.rs): the last run's
  largest bond and least retained weight, and the warning.
- [`MpsResumerError`](error.rs): why a shot failed, apart from a trap of
  its program.

A Bell pair, as the tests run it
([`src/tests/mps_simulator/resumer.rs`](../tests/mps_simulator/resumer.rs),
whose `run` is `expect_ready(resumer.run(&QirMachine::new(program), shots))`):

```rust
let mut resumer = MpsResumer::new(ExactReference::new(), 7);
let records = run(&mut resumer, &bell, &QirShotId::first(32)).expect("the program runs");
assert!(records.iter().all(|shot| shot[0] == shot[1]));
```

The cutoff below which an engine drops singular values is the engine's own
setting: the engine contract takes only a capacity (here
`MpsShape::qubit_chain(n, χ)`).

## Planned

- **Noise and loss on matrix product states:** a shot that reaches a
  correlated noise, readout noise or loss check call fails, naming this
  item, and a noise model set on a simulator has no counterpart here. Its
  case: a logical Bell test on 56 qubits with Pauli noise and loss.
- **The sampled form:** a program that does not continue after a batch
  run once, its readout drawn for every shot with `MpsEngine::sample`. Its
  case: sampling a Base Profile program's many shots at the cost of one.
- **Queries** of the state (expectations of Pauli sums, the chain's cost).
  Its case: a chemistry user's energy estimate ⟨H⟩ read without shots.
- **The Python surface** that reaches this resumer. Its case:
  `run_qir(type="mps")` and `pip install qdk[tensornetwork]`.
- **Groups run in parallel** on CPU threads. Its case: the polyene of the
  chemistry case, whose 1,000 shots split into many histories.

## Files

Paths are under `source/simulators/src/`.

| File                                             | Defines                                                                       |
| ------------------------------------------------ | ----------------------------------------------------------------------------- |
| `mps_simulator.rs`                               | the module's role and public surface                                          |
| `mps_simulator/resumer.rs`                       | `MpsResumer`, `DEFAULT_MAX_BOND_DIMENSION`, the grouped run                   |
| `mps_simulator/resumer/lowering.rs`              | a batch's calls as steps on sites                                             |
| `mps_simulator/resumer/order.rs`                 | the qubit order                                                               |
| `mps_simulator/resumer/splitmix64.rs`            | `SplitMix64`, the generator each shot draws from                              |
| `mps_simulator/fusion.rs`                        | `GateFusion`                                                                  |
| `mps_simulator/report.rs`                        | `MpsRunReport`, `TruncationWarning`, `TRUNCATION_TOLERANCE`                   |
| `mps_simulator/error.rs`                         | `MpsResumerError`                                                             |
| `tests/mps_simulator.rs`, `tests/mps_simulator/` | the tests, one file per file with behaviour; the programs and the test double |
