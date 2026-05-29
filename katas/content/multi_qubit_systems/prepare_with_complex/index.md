
**Input:** A two-qubit system in the basis state $\ket{00} = \begin{bmatrix} 1 \\ 0 \\ 0 \\ 0 \end{bmatrix}$.

**Goal:** Transform the system into the state $\frac{1}{2}\big(\ket{00} + e^{i\pi/4}\ket{01} + e^{i\pi/2}\ket{10} + e^{3i\pi/4}\ket{11}\big) = \frac{1}{2}\begin{bmatrix} 1 \\ e^{i\pi/4} \\ e^{i\pi/2} \\ e^{3i\pi/4} \end{bmatrix}$.

:::hint
Represent the target state as a tensor product $\frac{1}{\sqrt2}\big(\ket{0} + e^{i\pi/2}\ket{1}\big) \otimes \frac{1}{\sqrt2}\big(\ket{0} + e^{i\pi/4}\ket{1}\big) = \frac{1}{\sqrt2} \begin{bmatrix} 1 \\ e^{i\pi/2} \end{bmatrix} \otimes \frac{1}{\sqrt2}\begin{bmatrix} 1 \\ e^{i\pi/4} \end{bmatrix}$.
:::
