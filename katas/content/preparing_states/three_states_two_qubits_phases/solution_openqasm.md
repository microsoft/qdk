This solution builds on the three-states preparation and adds relative phases.

1. First, prepare the $(\ket{00} + \ket{01} + \ket{10})/\sqrt{3}$ state using `ry` and `negctrl @ h`, as in the previous exercise.
2. Apply `p(4π/3)` to the first qubit to add a phase of $e^{i \cdot 4\pi/3}$ to states where the first qubit is $\ket{1}$.
3. Apply `p(2π/3)` to the second qubit to add a phase of $e^{i \cdot 2\pi/3}$ to states where the second qubit is $\ket{1}$.

The combined effect produces the state $(\ket{00} + e^{i \cdot 2\pi/3}\ket{01} + e^{i \cdot 4\pi/3}\ket{10})/\sqrt{3}$, where each basis state has an equally spaced phase.

@[solution]({
    "id": "preparing_states__three_states_two_qubits_phases_solution_openqasm",
    "codePath": "./Solution.qasm"
})
