<h2 style="color:#D30982;">Part 1: QPE Algorithms</h2>

Two phase estimation strategies are available:

- **`iterative`** (IQPE) — uses 1 ancilla qubit and runs `num_bits` sequential single-qubit measurements. Each iteration applies the controlled unitary to a progressively lower power and reads one bit of the phase. `shots_per_bit` controls majority-vote reliability per bit. Low qubit overhead makes this the practical choice for simulation and near-term hardware.
- **`qiskit_standard`** — textbook QPE with `n` ancilla qubits measured simultaneously after an inverse QFT. Produces a single exportable full circuit (QASM/QIR), useful for circuit inspection and QASM/QIR export. Requires `n_ancilla + n_system` qubits. `qft_do_swaps` toggles the final swap layer. For Hamiltonians of the size used in this course, the resulting circuit is too deep for near-term hardware;
  `qiskit_standard` is included here for completeness and circuit inspection.

The key trade-off: IQPE uses fewer qubits at the cost of sequential classical post-processing; standard QPE parallelises measurement into one deep circuit at the cost of ancilla overhead. **For the Hamiltonians used in this course, `iterative` is the only practical choice.**

Unlike standalone IQPE implementations in other libraries (Qiskit, PennyLane), the `iterative` component here plugs directly into the full chemistry pipeline — the same Hamiltonian built in Chapter 4 flows into QPE without any manual wiring between steps.