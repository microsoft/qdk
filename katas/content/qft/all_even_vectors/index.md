**Input:** $n$ qubits in the $\ket{0 \dots 0}$ state.

**Goal:**  Change the state of the qubits to an equal superposition of all even basis vectors:

$$\frac1{\sqrt{2^{n-1}}} \big(\ket{0} + \ket{2} + ... + \ket{2^n-2} \big)$$

> For example, for $n = 2$ the goal state is $\frac1{\sqrt2} \big(\ket{0} + \ket{2}\big)$.

:::hint
Which superposition of two basis states can be mapped to this state using QFT?
    Use the solutions to earlier tasks in this lesson to figure out the answer.
:::