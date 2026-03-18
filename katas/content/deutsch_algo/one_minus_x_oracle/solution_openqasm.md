The phase oracle for $f(x) = 1 - x$ needs to apply a phase of $(-1)^{1-x}$: phase $-1$ when $x = 0$ and phase $+1$ when $x = 1$.

The sequence `x`, `z`, `x` achieves this. The `x` gates flip the qubit so that the `z` gate (which applies phase $-1$ to $\ket{1}$) instead applies phase $-1$ to $\ket{0}$. The resulting matrix is $XZX = -Z$, which maps $\ket{0} \to -\ket{0}$ and $\ket{1} \to \ket{1}$.

@[solution]({
    "id": "deutsch_algo__one_minus_x_oracle_solution_openqasm",
    "codePath": "./Solution.qasm"
})
