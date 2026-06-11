**Inputs:**

1. Angle $\alpha$, in radians, represented as a `Double`.
2. A qubit which is guaranteed to be in either the $\ket{A}$ or the $\ket{B}$ state, where $\ket{A} = \cos \alpha \ket{0} - i \sin \alpha \ket{1}$ and $\ket{B} = - i \sin \alpha \ket{0} + \cos \alpha \ket{1}$.

**Output:** `true` if the qubit was in the $\ket{A}$ state, or `false` if it was in the $\ket{B}$ state. The state of the qubit at the end of the operation does not matter.

:::hint
<p>An $R_x$ rotation can be used to go from the computational basis $\{ \ket 0, \ket 1 \}$ to the $\{ \ket{A}, \ket{B} \}$ basis and vice versa.</p>
:::
