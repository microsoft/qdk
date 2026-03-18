Measuring a qubit returns `0` for $\ket{0}$ and `1` for $\ket{1}$. Since we want to return `true` when the qubit is $\ket{0}$, we negate the boolean conversion of the measurement result.

@[solution]({
    "id": "single_qubit_measurements__distinguish_0_and_1_solution_openqasm",
    "codePath": "./Solution.qasm"
})
