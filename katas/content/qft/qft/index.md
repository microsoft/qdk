**Input**:
A register of $n$ qubits in state $\ket{j_1 j_2 ... j_n}$.

**Goal**: 
Apply quantum Fourier transform to the input register, that is, transform it to a state

$$\begin{align*}
&\frac1{\sqrt2} \big(\ket{0} + e^{2\pi i \cdot 0.j_n} \ket{1} \big) \otimes \\
\otimes &\frac1{\sqrt2} \big(\ket{0} + e^{2\pi i \cdot 0.j_{n-1} j_n} \ket{1} \big) \otimes ... \otimes \\
\otimes &\frac1{\sqrt2} \big(\ket{0} + e^{2\pi i \cdot 0.j_1 j_2 ... j_{n-1} j_n} \ket{1} \big)
\end{align*}$$

The register of qubits can be in superposition as well;
the behavior in this case is defined by behavior on the basis states and the linearity of unitary transformations.

> Notice that this implementation will let you use inverse QFT without writing any extra code, just by using the adjoint of this operation!

:::hint
Consider first preparing a state with the states of individual qubits reversed and then transforming it to the goal state.
:::