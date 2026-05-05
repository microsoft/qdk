<h2 style="color:#D30982;">Part 5: Verifying energy agreement and solver comparison</h2>

All encodings are unitary-equivalent representations of the same operator: their ground-state eigenvalues must agree exactly. This cell runs exact diagonalization on every encoding and checks agreement, then compares the dense and sparse matrix solvers.

The **dense solver** builds the full 2ⁿ× 2ⁿ Hilbert space matrix explicitly. This is simple but uses O(4ⁿ) memory. For 8 qubits (2⁸ = 256) it is fast; for 16 qubits (2¹⁶ = 65536) it is slow.

The **sparse solver** uses iterative Lanczos/Davidson methods, only storing the non-zero Hamiltonian entries. Much more efficient for larger qubit counts; the recommended default for n > 12 qubits.

In the cell below, choose which solver to use and then confirm all encodings agree. Make sure you replace the "???" with the matrix solver. 