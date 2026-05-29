
**Input:** 2 qubits in an unknown state $\ket{\psi} = \sum_{k = 0}^{3} x_k \ket{k}$. The amplitudes $x_k$ will be real and non-negative.

**Output:** A tuple of two numbers $(x_1', x_2')$ - your estimates of the amplitudes of the state $\ket{01}$ and $\ket{10}$, respectively.
The absolute errors $|x_1 - x_1'|$ and $|x_2 - x_2'|$ should be less than or equal to 0.001.

Please note that the state parameter is guaranteed to be the same
if you run the code several times. Your operation will be called
once for every run.

:::hint
On a physical quantum system, there would be no way to obtain these values from a single observation. Since this program runs on a simulator, you can use <code>DumpMachine</code> to inspect the qubits and take a note of their state. Furthermore, the problem statement guarantees that the state will be the same from invocation to invocation. So, you can update the code to return the amplitudes that you've taken note of, then run the code again.
:::
