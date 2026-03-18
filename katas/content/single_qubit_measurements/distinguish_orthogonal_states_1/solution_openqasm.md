The states $\ket{\psi_+} = 0.6\ket{0} + 0.8\ket{1}$ and $\ket{\psi_-} = -0.8\ket{0} + 0.6\ket{1}$ can be rotated back to the computational basis using `ry` with the negative of the rotation angle $\theta = \text{arctan}(0.8 / 0.6)$. After rotation, measuring in the computational basis distinguishes the two states.

@[solution]({
    "id": "single_qubit_measurements__distinguish_orthogonal_states_1_solution_openqasm",
    "codePath": "./Solution.qasm"
})
