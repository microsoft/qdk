Apply `h` to Alice's qubit to create a superposition, then `cx` (CNOT) to entangle Alice's and Bob's qubits, creating the Bell state $(\ket{00} + \ket{11})/\sqrt{2}$.

The two separate `qubit` declarations produce individual qubit parameters matching the Q# signature `(Qubit, Qubit)`.

@[solution]({
    "id": "teleportation__entangled_pair_solution_openqasm",
    "codePath": "./Solution.qasm"
})
