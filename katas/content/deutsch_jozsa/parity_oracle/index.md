**Input:** An array of $N$ qubits in an arbitrary state $\ket{x}$.

**Goal:** Apply the phase oracle $U_f$ for $f(x) = 1 \text{ if x has odd number of 1s, and } 0 \text{ otherwise }$ to the qubits.
That is, apply a relative phase $(-1)^{f(x)}$ to each basis state $\ket{x}$.

:::hint
Can you represent the function as a sum of expressions that depend on individual bits of $x$ (modulo $2$)? 
Can you then represent the effect of the oracle on the array of qubits as a product of effects on each qubit in the array?
:::
