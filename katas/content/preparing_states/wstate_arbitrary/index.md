**Input:** $N$ qubits in the $\ket{0 \dots 0}$ state ($N$ isn't necessarily a power of two).

**Goal:**  Change the state of the qubits to the [W state](https://en.wikipedia.org/wiki/W_state) - an equal superposition of $N$ basis states on $N$ qubits which have Hamming weight of 1.

For example, for $N = 3$ the required state is $\frac{1}{\sqrt{3}}\big(\ket{100} + \ket{010} + \ket{001}\big)$.

:::hint
You can modify the signature of the given operation to specify its controlled specialization.
:::
