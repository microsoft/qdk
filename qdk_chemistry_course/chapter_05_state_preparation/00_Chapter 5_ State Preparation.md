<h1 style="color:#D30982;text-align:center;">Chapter 5: State Preparation</h1>

<h2 style="color:#D30982;">What you'll learn</h2>

- The two isometry-based state preparers in the QDK and their settings
- Why a single Slater determinant (HF reference) requires only X gates — depth 1
- How circuit depth scales with the number of determinants in a multi-configurational state
- Why sparse isometry outperforms the regular (dense) isometry for CI/SCI wavefunctions
- How to export a prepared state circuit to QASM, QIR, Qiskit, and Q# formats

<h2 style="color:#D30982;">From wavefunction to quantum register</h2>

The previous chapters produced a classical wavefunction description: either a single Slater determinant (HF) or a superposition of many determinants (SCI). Before quantum phase estimation can run, this classical description must be encoded into an actual quantum circuit that maps |00...0⟩ to the target state.

This is **state preparation** (also called isometry synthesis). Both methods in the QDK are designed specifically for **CI/SCI wavefunctions** — states expressed as linear combinations of Slater determinants, exactly the representation produced by HF and all multi-configuration methods in Chapters 1–3. Other trial state representations (variational ansätze, tensor network states) are not supported by these preparers.

| Preparer | Strategy | When to use |
|---|---|---|
| `sparse_isometry_gf2x` | GF2X elimination on the sparse binary amplitude matrix | Any CI/SCI wavefunction; depth scales with number of determinants |
| `qiskit_regular_isometry` | Full dense unitary decomposition | Not practical for real CI/SCI states — depth scales with the full 2ⁿ Hilbert space regardless of sparsity |

The key distinction is **configuration count, not qubit count**: `sparse_isometry_gf2x` only touches the non-zero amplitudes in the wavefunction; `qiskit_regular_isometry` decomposes the full 2ⁿ-dimensional unitary even if the wavefunction has just a handful of determinants. For any realistic chemistry calculation, `sparse_isometry_gf2x` is the only practical choice.