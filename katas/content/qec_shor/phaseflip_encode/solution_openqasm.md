First apply the bit-flip encoding using two `cx` gates, then apply `h` to all three qubits to convert the bit-flip code into a phase-flip code, encoding $\alpha\ket{0} + \beta\ket{1}$ as $\alpha\ket{+++} + \beta\ket{---}$.

@[solution]({
    "id": "qec_shor__phaseflip_encode_solution_openqasm",
    "codePath": "./Solution.qasm"
})
