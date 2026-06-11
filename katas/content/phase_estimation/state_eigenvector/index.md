**Inputs:**

1. A single-qubit unitary $U$.
2. A single-qubit state $\ket{\psi}$, given as a unitary $P$ that prepares it from the $\ket{0}$ state. In other words, the result of applying the unitary $P$ to the state $\ket{0}$ is the $\ket{\psi}$ state:
   $$P\ket{0} = \ket{\psi}$$

**Output:** Return true if the given state is an eigenstate of the given unitary, and false otherwise.

<details>
  <summary><b>Need a hint?</b></summary>

The library operation <code>CheckZero</code> (from <code>Std.Diagnostics</code>) allows you to check whether the state of the given qubit is $\ket{0}$.

</details>
