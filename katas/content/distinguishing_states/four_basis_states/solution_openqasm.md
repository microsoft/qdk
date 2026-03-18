Measuring both qubits gives two classical bits. The first qubit gives the most significant bit and the second gives the least significant bit. Combining them as `c0 * 2 + c1` maps $\ket{00} \to 0$, $\ket{01} \to 1$, $\ket{10} \to 2$, $\ket{11} \to 3$.

@[solution]({
    "id": "distinguishing_states__four_basis_states_solution_openqasm",
    "codePath": "./Solution.qasm"
})
