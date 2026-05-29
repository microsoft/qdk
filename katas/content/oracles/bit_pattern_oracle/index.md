**Inputs:**

  1. $N$ qubits in an arbitrary state $\ket{x}$ (input/query register).
  2. A qubit in an arbitrary state $\ket{y}$ (target qubit).
  3. A boolean array of length $N$ `pattern` representing a basis state; `true` and `false` elements correspond to $\ket{1}$ and $\ket{0}$, respectively.

**Goal:**

Flip the state of $\ket{y}$ if the input register matches the basis state
represented by `pattern`.  

**Examples:**

* If the query register is in the state $\ket{010}$ and `pattern = [false, true, false]`, flip the state $\ket{y}$.
* If the query register is in the state $\ket{1001}$ and `pattern = [false, true, true, false]`, do nothing.
    
<br/>
<details>
  <summary><b>Before implementing this oracle, answer the question: are you implementing a marking or a phase oracle?</b></summary>
    This is a marking oracle, because you're flipping the state of the target qubit $\ket{y}$ based on the state of the input $\ket{x}$.
</details>

<br/>
:::hint
You need to flip the state of $\ket{y}$ if $\ket{x}$ matches the given pattern.  You may find the Q# library operation <code>ApplyControlledOnBitString</code> useful in your implementation.
:::
