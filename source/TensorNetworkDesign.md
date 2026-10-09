# Tensor-network simulation in the QDK: design

Draft of Oct 9, 2026.

## 0. Open questions

The users' answers shape the milestones, so they come first. None has been
sent yet. Each question names the milestones (M1, M2, …) and the needs (U1,
U2, …) its answer shapes.

### 0.1 For the QEC user

| #   | Question                                                                                                          | Shapes                                                                             |
| --- | ----------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| Q1  | Which noise model should the gadget benchmarks use: channels, rates and loss model?                               | M2: the noise drawn and applied (U3); M3: the rest of the noise model              |
| Q2  | Is offline decoding from the shot's records enough, or must a decoder steer the rest of the shot?                 | M2 leaves out a decoder steering the shot; if needed, it is a later milestone (U4) |
| Q3  | How many shots does a benchmark need, or which logical error rate must it resolve?                                | M2: the time per shot that is acceptable (U6)                                      |
| Q4  | Is a bounded repeat of Rz1 (6 levels) acceptable until the math functions (`cos`, `sin`, `arccos`) are supported? | M2: whether the logical Bell test needs the math functions first                   |
| Q5  | Is a logical-level H₂ QPE, with the gadgets' logical noise, what you want to report to the chemistry user?        | the combined problem, a later milestone; neither user has asked for it yet         |

### 0.2 For the chemistry user

| #   | Question                                                                                    | Shapes                                                                     |
| --- | ------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| C1  | Which examples come after H₂, and at what sizes (the walk-through you offered)?             | the scale beyond 30 qubits (U5) and the milestones after M1                |
| C2  | Which final-state quantities do you need, for example the QPE overlap $\lvert c_0\rvert^2$? | the final state (U7), not in M1–M3                                         |
| C3  | What shape should the `CircuitExecutor` in `qdk-chemistry` take?                            | the Python API on top of `tensornetwork_qir`                               |
| C4  | Does noise matter for your circuits?                                                        | whether M2's noise support is also for chemistry, or only for the QEC user |

### 0.3 For us

| #   | Question                                                                                                         | Shapes             |
| --- | ---------------------------------------------------------------------------------------------------------------- | ------------------ |
| T1  | How does `tensornetwork_qir` take the noise model in M2? Not designed yet.                                       | M2: the Python API |
| T2  | M3's efforts are estimated from the CPU simulator's code, not from the prototype; check them again when M2 ends. | M3: its schedule   |

## 1. The problem

The QEC user frames the two users' problems as one. The chemistry user's
quantum phase estimation (QPE) circuits, from `qdk-chemistry`, contain
non-Clifford rotations, each run fault-tolerantly as a gadget; the QEC user
simulates each gadget under noise and reports its logical performance, so
the chemistry user knows what to expect of his circuits. That needs two
simulations: the chemistry user's circuits, starting with H₂ and moving to
larger molecules, and the gadgets, starting with the Fire & Ice Rz1 gadget.
Both users want shots, as hardware gives them, and accept a controlled
approximation if its error is reported.

Each QDK simulator's cost grows with one property of the program: for dense
full state, the number of qubits, which caps it near 30; for sparse, the
number of non-zero amplitudes; for the branching stabilizer, the number of
live non-Clifford rotations. Qodec, new on `main`, adds no method: it drives
dense, branching-stabilizer and Clifford engines, and shares their limits.
Sparse runs H₂, but the gadget runs on none of them today, and larger
molecules are expected to be out of reach too. A gadget may involve one
rotation or several, depending on its construction; in the QEC user's words,
"the TN approach is essential when we use constructions with several 1Q
rotations".

## 2. Why tensor networks

Other alternatives could cover the problem, but each falls short:

| Alternative                                                                            | Why it falls short                                                                                    |
| -------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| Dense full state on larger hardware                                                    | the gadget's 56 live qubits need $2^{60}$ bytes, beyond any machine                                   |
| Sparse, through `run_qir`                                                              | the gadget's code states have exponentially many non-zero amplitudes: more than 10 minutes per shot   |
| Branching stabilizer, with random measurements absorbed into its Clifford frame        | the cost still doubles with each live rotation: it may fit gadgets with few, not those with several   |
| Other methods whose cost grows with the rotations (stabilizer rank, quasi-probability) | the same limit, and not in the QDK                                                                    |
| Clifford stand-ins for the rotations                                                   | they change what is measured: the gadget's non-Clifford behavior                                      |
| An external platform (CUDA-Q, Qiskit Aer)                                              | users leave the QDK: their Q# and QIR programs and noise models must be translated into another stack |

A tensor network's cost grows with entanglement instead. A matrix product
state (MPS) stores one tensor per qubit rather than all $2^n$ amplitudes:

$$
\underbrace{16\cdot 2^{n}\ \text{bytes}}_{\text{dense}}
\qquad\qquad
\underbrace{|\psi\rangle \approx \sum_{s_1\ldots s_n} A^{[1]}_{s_1} A^{[2]}_{s_2}\cdots A^{[n]}_{s_n}\,|s_1\ldots s_n\rangle}_{\text{MPS: } 32\,n\,\chi^{2}\ \text{bytes},\quad \chi\approx 2^{S}}
$$

Each $A^{[k]}_{s_k}$ is a $\chi\times\chi$ matrix, where the bond dimension
$\chi$ is set by the entanglement $S$ across the chain's most entangled cut:
each Bell pair across a cut doubles it. Memory therefore grows linearly with
the qubits and exponentially only with the entanglement. For the gadget,
dense needs $2^{60}$ bytes, while a bound gives $\chi\le4{,}096$: 28 GiB,
within one 80 GiB GPU; the actual $\chi$ is still to be measured. When
$\chi$ would pass a cap, the MPS truncates and reports the discarded weight,
the controlled approximation the users accept. It also draws mid-circuit
measurements directly from the state. We build it into the QDK on NVIDIA's
cuTensorNet, a library loaded at run time, so users keep their programs and
tools.

## 3. Approach

We work backwards from the users. Each milestone starts from a program a
user gave us and the check that tells them it works, and builds only what
that program needs:

- **Build on what already shows value.** A prototype, validated on an NVIDIA
  A100 and demoed to both users, who liked the direction, is the starting
  point. The parts the milestones in scope need are ported to the branch
  `domingom/tensor-network-simulation`, one commit per functional block
  (§4), ready to be reviewed as the first PRs; the rest waits until a
  milestone needs it.
- **About a month each.** Work that does not fit in a month is split, so
  value never waits for a later milestone.
- **Production quality.** Each milestone ships tested, documented and
  installable, for the API it exposes.
- **Independent.** A milestone relies on no later one.
- **Evidence revises the plan.** Each milestone's measurements and the
  users' feedback reshape the next ones; after about two months, M1 and M2
  have shown both users' value and its cost, and the plan is evaluated.

| Milestone | Delivers                                                                                                                          | For             | Checked by                                                  | Status    |
| --------- | --------------------------------------------------------------------------------------------------------------------------------- | --------------- | ----------------------------------------------------------- | --------- |
| M1        | H₂ QPE sampled on MPS without noise, installed with `pip install qdk[tensornetwork]`                                              | chemistry user  | the outputs peak where the chemistry user expects           | firm      |
| M2        | the Rz1 logical Bell test on MPS with Pauli noise and loss, scored from its logical parities, without a decoder steering the shot | QEC user        | without noise, the two logical outcomes match on every shot | firm      |
| M3        | parity with the QDK's simulators: the rest of the noise model; then `run_qir(type="mps")` makes MPS public                        | all QDK users   | a parity suite against the other `run_qir` types            | firm      |
| M4        | one execution path: the CPU, GPU and Clifford simulators move onto the execution core                                             | QDK maintainers | M3's parity suite                                           | proposal  |
| M5+       | a decoder steering the shot, if needed; the combined problem; the contraction method                                              | —               | —                                                           | proposals |

M1 to M3 use MPS. Other tensor-network methods plug in behind the same
interfaces when a problem needs them: exact contraction as a reference for
the MPS error, PEPS for 2D problems, tree tensor networks for larger codes,
and Clifford-augmented MPS for Clifford-heavy gadgets.

M1 comes first because H₂ needs the least beyond the prototype: one more
opcode, and no noise. The gadget also needs more opcodes, math functions
and noise.

Until M3, MPS is reached only through `tensornetwork_qir`, a new function
for tensor-network simulation: one call returns several results (samples,
expectation values, cost), which `run_qir` cannot do without changing its
existing callers, since it returns only shots. `run_qir` stays unchanged
until M3, where `run_qir(type="mps")` wraps the sampling of
`tensornetwork_qir`'s MPS method. M3 adds a simulator and changes nothing
that exists; M4 changes existing users' default paths, so it is its own
milestone, guarded by M3's parity suite.
