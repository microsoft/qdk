**Inputs:**

  1. A marking oracle implementing an unknown $N$-bit function $f(x)$.
  2. $N$ qubits in an arbitrary state (input/query register).
  
**Goal:**

Flip the phase of each basis state $\ket{x}$ for which $f(x) = 1$. You can only access $f(x)$ via the marking oracle you are given.

The operation you implement is the phase oracle for the same function.

<br/>
:::hint
Recall that you can allocate extra qubits to assist in this operation.  Is there a state that you could prepare with an auxiliary qubit which would help you to convert the marking oracle to a phase oracle?
:::
