This measurement-free teleportation protocol transfers the state of `qMessage` to `qBob` using only unitary gates (no measurements):

1. `cx qMessage, qAlice` — entangle the message with Alice's qubit.
2. `h qMessage` — apply Hadamard to the message qubit.
3. `ctrl @ z qMessage, qBob` — controlled-Z conditioned on the message qubit.
4. `ctrl @ x qAlice, qBob` — controlled-X conditioned on Alice's qubit.

The `ctrl @` modifier applies the gate when the control qubit is $\ket{1}$.

@[solution]({
    "id": "teleportation__measurement_free_teleportation_solution_openqasm",
    "codePath": "./Solution.qasm"
})
