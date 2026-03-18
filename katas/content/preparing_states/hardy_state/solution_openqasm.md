The Hardy state can be prepared using rotation gates with computed angles and conditional application using gate modifiers.

1. Apply `ry` with angle $2 \arccos\sqrt{10/12}$ to the first qubit to set up the initial amplitude distribution.
2. Use `negctrl @ ry` (controlled on $\ket{0}$) to rotate the second qubit by $2 \arccos(3/\sqrt{10})$ when the first qubit is $\ket{0}$.
3. Use `ctrl @ ry` (controlled on $\ket{1}$) to rotate the second qubit by $\pi/2$ when the first qubit is $\ket{1}$.

The `negctrl @` modifier applies the gate when the control qubit is in the $\ket{0}$ state, equivalent to Q#'s `ApplyControlledOnInt(0, ...)`. The `ctrl @` modifier applies when the control is $\ket{1}$.

@[solution]({
    "id": "preparing_states__hardy_state_solution_openqasm",
    "codePath": "./Solution.qasm"
})
