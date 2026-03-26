Apply `h` to the first qubit to create the superposition $\frac{1}{\sqrt{2}}(\ket{0\dots0} + \ket{1\dots0})$, then apply the QFT. By linearity, this produces the sum of $\text{QFT}\ket{0\dots0}$ (all basis vectors) and $\text{QFT}\ket{10\dots0}$ (alternating amplitudes), which equals an equal superposition of all even basis vectors.

@[solution]({
    "id": "qft__all_even_vectors_solution_openqasm",
    "codePath": "./Solution.qasm"
})
