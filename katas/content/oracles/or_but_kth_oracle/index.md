**Inputs:**

  1. $N \geq 2$ qubits in an arbitrary state $\ket{x}$ (input/query register).
  2. An integer $k$ such that $0 \leq k < N$.

**Goal:**

Flip the sign of the basis state $\ket{x}$ if any of the bits of $x$ (not considering the $k$-th bit) are $1$ in the input register. In other words, the input register with the $k$-th qubit excluded shouldn't be in the all zero state to flip the sign of the input register. The state of the $k$-th qubit does not affect the result.

*Feel free to explore implementing this operation with or without auxiliary qubits.*

**Examples:**

* If the query register is in the state $\ket{010}$ and $k=0$, flip the sign of the register.
* If the query register is in the state $\ket{010}$ and $k=1$, do nothing.

<details>
  <summary><b>Before implementing this oracle, answer the question: are you implementing a marking or a phase oracle?</b></summary>
    This is a phase oracle, because you're changing the phase of the input state $\ket{x}$ based on the value of the function $f(x)$.
</details>

:::hint
You can use the previously implemented oracles if needed by copying the code.
<br/>
You can use <a href="https://learn.microsoft.com/azure/quantum/user-guide/language/expressions/itemaccessexpressions#array-item-access-and-array-slicing" target="_blank">array slicing</a> to get parts of the array before and after the $k$-th element.
:::
