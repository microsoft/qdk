**Inputs:**

1. `qAlice` : Qubit received from Alice.
2. `qBob` : Bob's part of the entangled pair.

**Goal** :  Decode the message using the qubit received from Alice. For this, retrieve two bits of classical data from the qubits and return a tuple of the form `(Bool, Bool)` to represent `message`. The state of the qubits in the end of the operation does not matter.

Note that your solution is tested as part of an end-to-end implementation of the protocol; the goal is to get the message transmitted correctly.
