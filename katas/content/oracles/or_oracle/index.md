**Inputs:**

  1. $N$ qubits in an arbitrary state $\ket{x}$ (input/query register).
  2. A qubit in an arbitrary state $\ket{y}$ (target qubit).

**Goal:**

Flip the state of $\ket{y}$ if the input register is in any basis state
except for $\ket{00...0}$ (the all zero state).

**Examples:**

* If the query register is in the state $\ket{10000001}$, $\ket{11101101}$ or $\ket{0010101}$, flip the state $\ket{y}$.
* If the query register is in the state $\ket{000}$, do nothing.

<br/>
<details>
  <summary><b>Before implementing this oracle, answer the question: are you implementing a marking or a phase oracle?</b></summary>
    This is a marking oracle, because you're flipping the state of the target qubit $\ket{y}$ based on the state of the input $\ket{x}$.
</details>

<br/>
:::hint
You need to flip the state of $\ket{y}$ for every input except $\ket{00...0}$, or, alternatively, flip it unconditionally and then flip it for the $\ket{00...0}$ state.   You may find the Q# library function <code>ApplyControlledOnInt</code> useful in your implementation.
:::
