**Inputs:**

1. A single-qubit unitary $U$ that is guaranteed to have an eigenvalue $+1$ or $-1$ 
(with eigenphases $0$ or $\frac12$, respectively). The unitary will have its controlled variant defined.
2. A single-qubit state $\ket{\psi}$, given as a unitary $P$ that prepares it from the $\ket{0}$ state. In other words, the result of applying the unitary $P$ to the state $\ket{0}$ is the $\ket{\psi}$ state:
   $$P\ket{0} = \ket{\psi}$$

**Output:** Return the eigenvalue which corresponds to the eigenstate $\ket{\psi}$ ($+1$ or $-1$).

<details>
  <summary><b>Need a hint?</b></summary>
  You can do this by allocating exactly two qubits and calling <code>Controlled U</code> exactly once.
</details>
