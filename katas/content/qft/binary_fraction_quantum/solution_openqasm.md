The quantum version replaces the classical condition on each bit $j_k$ with a controlled phase gate `ctrl @ p`, using each qubit of register $j$ as the control.

For a 2-qubit register $j$, apply controlled phase rotations `ctrl @ p(π)` and `ctrl @ p(π/2)` with $j[0]$ and $j[1]$ as controls, respectively. These correspond to the binary fractions $0.j_1$ and $0.0j_2$.

@[solution]({
    "id": "qft__binary_fraction_quantum_solution_openqasm",
    "codePath": "./Solution.qasm"
})
