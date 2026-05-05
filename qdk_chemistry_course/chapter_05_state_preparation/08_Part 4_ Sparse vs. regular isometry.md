<h2 style="color:#D30982;">Part 4: Sparse vs. regular isometry</h2>

`sparse_isometry_gf2x` uses <a href="https://journals.aps.org/pra/abstract/10.1103/PhysRevA.93.032318" target="_blank">GF2X</a> (Gaussian elimination over GF(2) augmented with X operations) to find a compact CNOT+X circuit. It exploits the sparsity of the CI/SCI wavefunction — only the non-zero amplitudes (the occupied determinants) contribute to the circuit. Depth grows linearly with the number of determinants, not with 2ⁿ.

`qiskit_regular_isometry` performs a full dense unitary decomposition. It does not exploit sparsity: its cost scales with the full 2ⁿ-dimensional Hilbert space regardless of how many determinants the wavefunction has. A wavefunction with 10 determinants on 16 qubits costs the same as a wavefunction with 65,536 determinants. For the same 10-determinant state: depth 314 (sparse) vs. 337,756 (regular).

The practical takeaway: the distinction between these two methods is **not about qubit count**, but it is about whether the method exploits the sparse structure of CI/SCI wavefunctions. For any chemistry calculation in this course, `sparse_isometry_gf2x` is the correct choice.

**Runtime note:** `qiskit_regular_isometry` decomposes a full 2¹⁶-dimensional unitary — expect ~5 minutes on most laptops.

Fill in the preparer name to complete the comparison, in the cell below. 