<h2 style="color:#D30982;">Part 1: Building the active space Hamiltonian</h2>

The `hamiltonian_constructor` takes an `Orbitals` object (from any wavefunction) and computes the one- and two-body integrals in that orbital basis. The result is a classical Hamiltonian object that stores everything needed for both exact diagonalization and qubit mapping.

`get_summary()` exposes four diagnostically useful fields: active orbital count (qubit cost), core energy (constant offset folded into the qubit identity term), and the one- and two-body integral counts with a sparsity indicator (how many are above a threshold).