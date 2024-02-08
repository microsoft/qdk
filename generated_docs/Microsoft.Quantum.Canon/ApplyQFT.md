# operation ApplyQFT(qs : Qubit[]) : Unit is Adj + Ctl

## Summary
Applies Quantum Fourier Transform (QFT) to a little-endian quantum register.

## Description
Applies QFT to a little-endian register `qs` of length n
containing |x₁⟩⊗|x₂⟩⊗…⊗|xₙ⟩. The qs[0] contains the
least significant bit xₙ. The state of qs[0] becomes
(|0⟩+𝑒^(2π𝑖[0.xₙ])|1⟩)/sqrt(2) after the operation.

## Input
### qs
Quantum register in a little-endian format to which the QFT is applied.

## Reference
 - [Quantum Fourier transform](https://en.wikipedia.org/wiki/Quantum_Fourier_transform)
