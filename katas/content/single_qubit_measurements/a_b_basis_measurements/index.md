**Inputs:**

1. Angle $\alpha$, in radians, represented as a `Double`.
1. A qubit in some unknown state.

**Output:** Implement a measurement in the $\{\ket A, \ket B\}$ basis. Same as in the previous exercise, $\ket{A} = \cos \alpha \ket{0} - i \sin \alpha \ket{1}$ and $\ket{B} = - i \sin \alpha \ket{0} + \cos \alpha \ket{1}$. Return `Zero` if the measurement outcome is $A$, and `One` if the outcome is $B$.
The state of the qubit after the measurement should correspond to the measurement result.

:::hint
<p>An $R_x$ rotation can be used to go from the computational basis $\{ \ket 0, \ket 1 \}$ to the $\{ \ket{A}, \ket{B} \}$ basis and vice versa.</p>
:::
