**Input:** $N = 2^k$ qubits in the $\ket{0 \dots 0}$ state.

**Goal:**  Change the state of the qubits to the [W state](https://en.wikipedia.org/wiki/W_state) - an equal superposition of $N$ basis states on $N$ qubits which have Hamming weight of 1.

For example, for $N = 4$ the required state is $\frac{1}{2}\big(\ket{1000} + \ket{0100} + \ket{0010} + \ket{0001}\big)$.

:::hint
You can use <code>Controlled</code> modifier to perform arbitrary controlled gates.
:::
