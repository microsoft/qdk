The number 7 in binary is $111$ — all three input qubits must be $\ket{1}$. This means we need a multiply-controlled X gate: flip the target qubit only when all three inputs are $\ket{1}$.

In OpenQASM 3.0, `ctrl(3) @ x` applies an X gate with 3 control qubits — the target is flipped only when all controls are in the $\ket{1}$ state. This is equivalent to Q#'s `Controlled X(x, y)`.

The two qubit declarations (`qubit[3] inp` and `qubit target`) map to the two operation parameters `(Qubit[], Qubit)`, matching the oracle signature.

@[solution]({
    "id": "oracles__marking_oracle_seven_solution_openqasm",
    "codePath": "./Solution.qasm"
})
