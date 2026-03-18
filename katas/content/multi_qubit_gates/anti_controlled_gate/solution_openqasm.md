First apply `x` to the target qubit to flip it, then apply `cx` (CNOT) to flip it back when the control is $\ket{1}$. This effectively applies X to the target when the control is $\ket{0}$.

@[solution]({
    "id": "multi_qubit_gates__anti_controlled_gate_solution_openqasm",
    "codePath": "./Solution.qasm"
})
