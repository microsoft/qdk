<h1 style="color:#D30982;text-align:center;">Chapter 4: Hamiltonian Construction & Qubit Mapping</h1>

<h2 style="color:#D30982;">What you'll learn</h2>

- What information the classical Hamiltonian encodes and how integral counts scale with active space size
- How to build Hamiltonians for different active spaces and read `get_summary()` diagnostically
- The three qubit encodings available in the QDK: Jordan-Wigner, Bravyi-Kitaev, and Parity
- How to verify that all encodings are equivalent via exact diagonalization
- How the Schatten norm connects your qubit Hamiltonian directly to QPE circuit cost
- How to build model Hamiltonians (Ising, Heisenberg, Hubbard) using `LatticeGraph` for hardware benchmarking

<h2 style="color:#D30982;">From active space to qubits</h2>

Chapter 3 produced an active space (via autoCAS-EOS) and showed that choosing fewer orbitals saves qubits. This chapter takes the next step: converting that active space into a qubit Hamiltonian that a quantum computer can act on.

The conversion has two stages. First, the classical Hamiltonian is constructed from the one- and two-body integrals of the active orbital basis. Second, a qubit mapper applies a fermion-to-qubit encoding that rewrites fermionic creation and annihilation operators as Pauli strings. Different encodings distribute fermionic anti-commutation relations differently across qubits — they are unitarily equivalent but can differ in circuit depth for specific hardware.