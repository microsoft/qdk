**Input:** A qubit which is guaranteed to be in either the $\ket {\psi_+}$ or the $\ket{\psi_-} $ state, where $\ket {\psi_+} = 0.6\ket 0 + 0.8 \ket 1 $ and $\ket {\psi_-} = -0.8\ket 0 + 0.6 \ket 1$.

**Output:** `true` if the qubit was in the $\ket {\psi_+}$ state, or `false` if it was in the $\ket{\psi_-} $ state. The state of the qubit at the end of the operation does not matter.

:::hint
A suitable $R_y$ rotation can be used to go from the computational basis $\{ \ket 0, \ket 1 \}$ to the $\{ \ket{\psi_+}, \ket{\psi_-} \}$ basis and vice versa.
:::
