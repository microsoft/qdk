**Inputs:**
1. $N$ ($N \ge 1$) qubits in the $\ket{0 \dots 0}$ state.
2. An `Int` `parity`.

**Goal:** Change the state to an equal superposition of all basis states that have
* an even number of 1s in them if `parity = 0`, or
* an odd number of 1s in them if `parity = 1`.

> For example, for $N = 2$ the required state is $\frac{1}{\sqrt{2}}\big(\ket{00} + \ket{11}\big)$ if `parity = 0`, or $\frac{1}{\sqrt{2}}\big(\ket{01} + \ket{10}\big)$ if `parity = 1`.

:::hint
Remember that you can call the solution recursively.
:::
