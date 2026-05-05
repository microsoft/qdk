<h2 style="color:#D30982;">Summary</h2>

In this chapter you:
- Inspected `iterative` QPE (the practical choice for this course) and `qiskit_standard` (for circuit export)
- Inspected three time-evolution builders (Trotter, qDRIFT, partially-randomized) and their step-term counts
- Inspected all circuit executors and the `pauli_sequence` controlled circuit mapper
- Ran IQPE end-to-end and read `QpeResult`: `raw_energy`, `bitstring_msb_first`, alias `branching`
- Inspected IQPE iteration circuit depths (simulator gate counts for the N₂ autoCAS-EOS system) — halving from ~208k to ~26k over 4 iterations
- Observed QPE error scaling: 401 mHa (4 bits) → 97 mHa (6) → 21 mHa (8) → 1.7 mHa (10)

**End-to-end pipeline complete:**
Structure (Ch.0–1) → HF/SCF (Ch.2) → active space (Ch.3) → qubit Hamiltonian (Ch.4) → state prep (Ch.5) → **QPE** → ground state energy

**Where this is heading:** The pipeline you ran here targets fault-tolerant hardware. For the first demonstration of an end-to-end QPE calculation with quantum error correction on real hardware, see <a href="https://arxiv.org/abs/2505.09133" target="_blank">Babbush et al. (2025)</a>.