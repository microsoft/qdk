<h1 style="color:#D30982;text-align:center;">Chapter 6: Quantum Phase Estimation</h1>

<h2 style="color:#D30982;">What you'll learn</h2>

- The `iterative` QPE strategy and its settings; `qiskit_standard` for circuit export and inspection
- Three time-evolution builders and their step-term trade-offs
- How to configure executors and circuit mappers
- Running IQPE end-to-end: reading `QpeResult`, alias branches, iteration circuit depths
- How QPE error scales with the number of phase bits

<a href="https://arxiv.org/abs/quant-ph/9511026" target="_blank">Quantum Phase Estimation</a> (QPE) is the canonical quantum algorithm for computing molecular ground-state energies. It measures the phase of the eigenvalue $e^{-i H t}$ accumulated under time evolution $U = e^{-i H t}$ — directly extracting the energy $E$ from the qubit Hamiltonian built in Chapter 4.


The full QPE pipeline requires four components:

| Component | Registry key | Role |
|---|---|---|
| `PhaseEstimation` | `"phase_estimation"` | Algorithm (IQPE or standard QPE) |
| `TimeEvolutionBuilder` | `"time_evolution_builder"` | Decomposes H into native gates |
| `ControlledEvolutionCircuitMapper` | `"controlled_evolution_circuit_mapper"` | Wraps evolution as controlled-U |
| `CircuitExecutor` | `"circuit_executor"` | Runs circuits on simulator or hardware |