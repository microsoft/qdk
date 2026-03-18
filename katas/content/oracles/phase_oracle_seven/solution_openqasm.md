The number 7 in binary is $111$ — all three qubits must be $\ket{1}$ for the phase to flip. The `ctrl(2) @ z` gate applies a Z gate with two additional control qubits, meaning the phase flip occurs only when all three qubits are in the $\ket{1}$ state.

This is equivalent to Q#'s `Controlled Z(Most(x), Tail(x))`.

@[solution]({
    "id": "oracles__phase_oracle_seven_solution_openqasm",
    "codePath": "./Solution.qasm"
})
