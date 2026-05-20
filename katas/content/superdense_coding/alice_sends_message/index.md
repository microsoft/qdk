**Inputs**:

1. `qAlice` : Alice's part of the entangled pair of qubits.
2. `message`: Two classical bits represented by a tuple of two `Bool` variables to represent `bit1` and `bit2` respectively.

**Goal**:
Encode the message (two classical bits) by manipulating Alice's qubit.

Superdense coding protocol changes the joint state of the two qubits to one of the following four states based on the value of message:

- `(0, 0)`: $\ket{\Phi^{+}} = \frac{1}{\sqrt{2}} (\ket{00} + \ket{11})$
- `(0, 1)`: $\ket{\Psi^{+}} = \frac{1}{\sqrt{2}} (\ket{01} + \ket{10})$
- `(1, 0)`: $\ket{\Phi^{-}} = \frac{1}{\sqrt{2}} (\ket{00} - \ket{11})$
- `(1, 1)`: $\ket{\Psi^{-}} = \frac{1}{\sqrt{2}} (\ket{01} - \ket{10})$

Note that your solution is tested as part of an end-to-end implementation of the protocol; the goal is to get the message transmitted correctly.
