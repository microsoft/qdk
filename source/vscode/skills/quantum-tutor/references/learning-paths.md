# Learning Paths

Curated, ordered subsets of katas organized by experience level. Each path lists kata IDs and their exercises in the recommended order.

Kata IDs reference folders under `katas/content/`. Exercise IDs are the subfolder names within each kata.

## Language Availability

All exercises are available in **Q#**. A subset of exercises also have **OpenQASM** variants. When the user chooses OpenQASM, exercises with an OpenQASM variant use `solution.qasm` files; exercises without a variant fall back to Q#.

Katas with OpenQASM variants (indicated with ⚡ below):

- `getting_started` — `flip_qubit`
- `single_qubit_gates` — `state_flip`, `sign_flip`, `basis_change`, `y_gate`, `sign_flip_on_zero`, `prepare_minus`, `global_phase_minusone`
- `multi_qubit_gates` — `entangle_qubits`, `preparing_bell_state`, `toffoli_gate`
- `preparing_states` — `plus_state`, `minus_state`, `bell_state`

The beginner path has the best OpenQASM coverage. Intermediate and advanced paths are currently Q#-only.

## Beginner Path

For users with no prior quantum computing knowledge. Covers fundamental concepts: what qubits are, basic gates, measurement, and simple protocols. This path has the best OpenQASM coverage — ideal for users who want to learn with OpenQASM.

| #   | Kata                        | Topic                                      | OpenQASM |
| --- | --------------------------- | ------------------------------------------ | -------- |
| 1   | `getting_started`           | Your first quantum program                 | ⚡       |
| 2   | `qubit`                     | Understanding qubits and quantum states    |          |
| 3   | `single_qubit_gates`        | X, H, Z, S, T and other single-qubit gates | ⚡       |
| 4   | `multi_qubit_systems`       | Working with multiple qubits               |          |
| 5   | `multi_qubit_gates`         | CNOT, SWAP, and controlled gates           | ⚡       |
| 6   | `preparing_states`          | Preparing specific quantum states          | ⚡       |
| 7   | `single_qubit_measurements` | Measuring single qubits                    |          |

## Intermediate Path

For users who understand qubits and basic gates. Covers measurement, state discrimination, and fundamental quantum protocols.

| #   | Kata                       | Topic                                  |
| --- | -------------------------- | -------------------------------------- |
| 1   | `multi_qubit_measurements` | Multi-qubit measurement techniques     |
| 2   | `distinguishing_states`    | Distinguishing quantum states          |
| 3   | `distinguishing_unitaries` | Identifying unknown unitary operations |
| 4   | `random_numbers`           | Quantum random number generation       |
| 5   | `teleportation`            | Quantum teleportation protocol         |
| 6   | `superdense_coding`        | Superdense coding protocol             |
| 7   | `key_distribution`         | Quantum key distribution (BB84)        |

## Advanced Path

For users ready for quantum algorithms, oracles, and error correction.

| #   | Kata                     | Topic                                |
| --- | ------------------------ | ------------------------------------ |
| 1   | `oracles`                | Quantum oracles                      |
| 2   | `marking_oracles`        | Marking oracles for search           |
| 3   | `deutsch_algo`           | Deutsch's algorithm                  |
| 4   | `deutsch_jozsa`          | Deutsch-Jozsa algorithm              |
| 5   | `grovers_search`         | Grover's search algorithm            |
| 6   | `solving_sat`            | Solving SAT with Grover's            |
| 7   | `solving_graph_coloring` | Graph coloring with quantum search   |
| 8   | `qft`                    | Quantum Fourier Transform            |
| 9   | `phase_estimation`       | Quantum phase estimation             |
| 10  | `qec_shor`               | Quantum error correction (Shor code) |

## Full Path

All katas in order, following `katas/content/index.json`. Use when the user wants the complete curriculum.

## Custom Path

If the user has specific interests (e.g., "I only care about Grover's algorithm"), build a custom path by selecting relevant katas. Include prerequisites — for example, Grover's search requires understanding oracles, which requires understanding multi-qubit gates.

### Prerequisite Graph

```text
getting_started
  └─► qubit
       └─► single_qubit_gates
            └─► multi_qubit_systems
                 └─► multi_qubit_gates
                      ├─► preparing_states
                      ├─► single_qubit_measurements
                      │    └─► multi_qubit_measurements
                      │         ├─► distinguishing_states
                      │         │    └─► distinguishing_unitaries
                      │         ├─► random_numbers
                      │         ├─► teleportation
                      │         ├─► superdense_coding
                      │         └─► key_distribution
                      └─► oracles
                           └─► marking_oracles
                                ├─► deutsch_algo
                                │    └─► deutsch_jozsa
                                └─► grovers_search
                                     ├─► solving_sat
                                     └─► solving_graph_coloring
                                          └─► qft
                                               └─► phase_estimation
                                                    └─► qec_shor
```
