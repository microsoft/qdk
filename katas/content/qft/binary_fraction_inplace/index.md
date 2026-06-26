**Input**:
A register of $n$ qubits in state $\ket{j_1 j_2 ... j_n}$.

**Goal**: 
Change the state of the register from $\ket{j_1} \otimes \ket{j_2 ... j_n}$ to $\frac1{\sqrt2}(\ket{0} + e^{2\pi i \cdot 0.j_1 j_2 ... j_n} \ket{1}) \otimes \ket{j_2 ... j_n}$.

:::hint
This task is very similar to the previous task, but the digit $j_1$ has to be encoded in-place. You can do this using the first task of the kata, "Single-Qubit QFT".
:::
