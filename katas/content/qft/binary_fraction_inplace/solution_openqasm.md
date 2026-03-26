Apply `h` to the first qubit to create the superposition $\frac{1}{\sqrt{2}}(\ket{0} + e^{2\pi i \cdot 0.j_1}\ket{1})$, then use controlled phase gates `ctrl @ p` with the remaining qubits as controls to add the binary fraction phase terms.

@[solution]({
    "id": "qft__binary_fraction_inplace_solution_openqasm",
    "codePath": "./Solution.qasm"
})
