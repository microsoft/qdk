Prepare the input state using `x`, `h`, `t`, and `inv @ t` gates to create the superposition $\frac{1}{\sqrt{2}}(e^{-i\pi/4}\ket{01\dots0} + e^{i\pi/4}\ket{11\dots0})$, then apply the QFT. The phase factors cancel out the unwanted imaginary component, producing the square wave pattern with alternating pairs of $+1$ and $-1$ amplitudes.

@[solution]({
    "id": "qft__square_wave_solution_openqasm",
    "codePath": "./Solution.qasm"
})
