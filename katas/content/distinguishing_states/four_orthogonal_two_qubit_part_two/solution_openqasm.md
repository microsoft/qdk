These states are entangled, so we first apply Hadamard to the second qubit, then CNOT to disentangle them, and finally Hadamard on the first qubit to convert to the computational basis. The measurement results are inverted: `(1 - c1) * 2 + (1 - c0)` gives the correct state index.

@[solution]({
    "id": "distinguishing_states__four_orthogonal_two_qubit_part_two_solution_openqasm",
    "codePath": "./Solution.qasm"
})
