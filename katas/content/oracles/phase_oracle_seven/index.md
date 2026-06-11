**Input:**
  3 qubits in an arbitrary state $\ket{x}$ (input/query register).

**Goal:**

Flip the sign of the input state $\ket{x}$ if the input register is in
the state $\ket{111}$ (encoding the integer $7$), and leave the input register unchanged otherwise.  
Don't allocate extra qubits to perform this operation.

**Examples:**

* If the query register is in the state $\ket{111}$, flip its sign.
* If the query register is in the state $\ket{010}$ or $\ket{101}$, do nothing.

:::hint
To solve this problem, you need to find a gate that will only flip the sign of the $\ket{111}$ basis state.  Which single-qubit gate flips the sign of the basis state $\ket{1}$ but not $\ket{0}$? How can you modify this gate to solve this problem?
:::
