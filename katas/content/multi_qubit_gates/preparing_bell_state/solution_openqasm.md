To prepare a Bell state, first apply the Hadamard gate `h` to the first qubit to create a superposition, then apply `cx` (CNOT) to entangle the two qubits. This produces the state $\frac{1}{\sqrt{2}}(\ket{00} + \ket{11})$.

@[solution]({
    "id": "multi_qubit_gates__preparing_bell_state_solution_openqasm",
    "codePath": "./Solution.qasm"
})
