**Inputs:** 

1. $N$ qubits in an arbitrary state $\ket{x}$ (input/query register).
2. A qubit in an arbitrary state $\ket{y}$ (output/target qubit).

**Goal:** 
Implement a marking oracle which calculates the parity of the input array, that is, the function
$$f(x) = 1 \text{ if } x \text{ has an odd number of 1s, and } 0 \text{ otherwise }$$

In other words, for each basis state $\ket{x}$, flip the state of the target qubit $\ket{y}$ if $f(x) = 1$, and leave it unchanged otherwise.
Leave the qubits in the input register in the same state they started in. 
Your solution should work on inputs in superposition, and not use any measurements.

:::hint
$f(x)$ can be represented as $x_0 \oplus x_1 \oplus ... \oplus x_{N-1}$.
:::
