To prepare a Bell state, apply the Hadamard gate `h` to the first qubit then a CNOT gate `cx` with the first qubit as control and the second as target. This creates the entangled state $\frac{1}{\sqrt{2}}(\ket{00} + \ket{11})$.

@[solution]({
    "id": "preparing_states__bell_state_solution_openqasm",
    "codePath": "./Solution.qasm"
})
