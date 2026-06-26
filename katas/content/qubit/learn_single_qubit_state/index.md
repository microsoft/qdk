
**Input:** A qubit in an unknown state $\ket{\psi} = \alpha\ket{0} + \beta\ket{1}$. The amplitudes $\alpha$ and $\beta$ will be real and non-negative.

**Output:** A tuple of two numbers $(\alpha', \beta')$ - your estimates of the amplitudes $\alpha$ and $\beta$.
The absolute errors $|\alpha - \alpha'|$ and $|\beta - \beta'|$ should be less than or equal to 0.001.

Please note that the state parameter is guaranteed to be the same
if you run the code several times. Your operation will be called
once for every run.

:::hint
On a physical quantum system, there's no way to obtain the values of $\alpha$ and $\beta$ from a single observation. Since this program runs on a simulator, you can use <code>DumpMachine</code> to inspect the qubit and take a note of its state. Furthermore, the problem statement guarantees, that the state will be the same from invocation to invocation. So you can update the code to return the amplitudes that you've taken note of. Then run the code again.
:::
