To measure in the X basis, we apply a Hadamard gate first. This maps $\ket{+}$ to $\ket{0}$ and $\ket{-}$ to $\ket{1}$. Measuring in the computational basis after the Hadamard then distinguishes the two states: the result is `1` (true) for $\ket{-}$.

@[solution]({
    "id": "single_qubit_measurements__distinguish_plus_and_minus_solution_openqasm",
    "codePath": "./Solution.qasm"
})
