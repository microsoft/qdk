**Input:** A qubit in an arbitrary state $\ket{\psi} = \alpha\ket{0} + \beta\ket{1}$.

**Goal:** Apply the phase oracle $U_f$ for $f(x) = 1 - x$ to the qubit.
That is, apply a relative phase $(-1)^{f(x)}$ to each basis state $\ket{x}$.

:::hint
You can represent the effect of the oracle as

$$U_f \ket{x} = (-1)^{1-x} \ket{x} = (-1) \cdot (-1)^x \ket{x}$$

Can you get this effect by combining some of the previous oracles implementations?
:::
