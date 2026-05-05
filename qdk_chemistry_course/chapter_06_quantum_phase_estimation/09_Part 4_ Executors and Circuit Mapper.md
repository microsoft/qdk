<h2 style="color:#D30982;">Part 4: Executors and Circuit Mapper</h2>

The `CircuitExecutor` runs each iteration circuit on a backend. Three executors are available:

- **`qdk_sparse_state_simulator`** — sparse statevector; efficient for ≤~20 qubits. The default choice for this course.
- **`qdk_full_state_simulator`** — dense statevector; builds the full 2ⁿ vector in memory. Only practical for <14 qubits.
- **`qiskit_aer_simulator`** — Qiskit Aer backend; supports `QuantumErrorProfile` noise models for realistic hardware noise simulation.

The only available circuit mapper is `pauli_sequence`, which expresses each exp(−iθP/2) term as a CNOT ladder + Rz rotation wrapped with a control qubit.