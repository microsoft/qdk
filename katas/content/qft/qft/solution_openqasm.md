The QFT circuit applies the binary fraction in-place transformation to each qubit in sequence, then reverses the qubit order with `swap` gates.

For each qubit $j[k]$, apply `h` followed by controlled phase gates `ctrl @ p(π/2^m)` using subsequent qubits as controls. After processing all qubits, swap the first and last qubits to reverse the register.

@[solution]({
    "id": "qft__qft_solution_openqasm",
    "codePath": "./Solution.qasm"
})
