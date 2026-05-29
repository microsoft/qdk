**Input:**

1. `controls` - a register of $N$ qubits in an arbitrary state $\ket{\phi}$.
2. `target` - a qubit in an arbitrary state $\ket{\psi}$.
3. `controlBits` - an array of $N$ booleans, specifying what state each control qubit should be in order to apply the gate.

**Goal:** Apply the controlled $X$ gate with the `controls` as control qubits and `target` as target, with the state specified by `controlBits` as controls. If the element of the array is `true`, the corresponding qubit is a regular control (should be in state $\ket{1}$), and if it is `false`, the corresponding qubit is an anti-control (should be in state $\ket{0}$).

> For example, if `controlBits = [true, false, true]`, the controlled $X$ gate should only be applied if the control qubits are in state $\ket{101}$.

:::hint
<p>Consider using a library operation for this task. If you want to do it without a library operation, don't forget to reset the qubits back to the state they were originally in.</p>
:::
