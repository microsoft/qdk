This solution uses the rotation approach (Solution B from the Q# version) rather than the measurement-based approach, since it produces the same state deterministically.

1. Apply `ry` with angle $2 \arcsin(1/\sqrt{3})$ to the first qubit. This creates the state $\sqrt{2/3}\ket{0} + \sqrt{1/3}\ket{1}$.
2. Use `negctrl @ h` to apply a Hadamard to the second qubit conditioned on the first qubit being $\ket{0}$. This splits the $\ket{0}$ amplitude equally between $\ket{00}$ and $\ket{01}$, producing $(\ket{00} + \ket{01} + \ket{10})/\sqrt{3}$.

The `negctrl @` modifier applies the gate when the control qubit is in the $\ket{0}$ state.

@[solution]({
    "id": "preparing_states__three_states_two_qubits_solution_openqasm",
    "codePath": "./Solution.qasm"
})
