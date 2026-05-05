<h2 style="color:#D30982;">Part 3: What qubit mappers are available?</h2>

Three fermion-to-qubit encodings are available in the QDK:

**<a href="https://www.tandfonline.com/doi/abs/10.1080/00268976.2011.552441" target="_blank">Jordan-Wigner</a>**: maps each spin-orbital to one qubit; the anti-commutation relation is enforced by Z-string parity chains. Intuitive, but Z-strings grow with system size.

**<a href="https://aip.scitation.org/doi/10.1063/1.4768229" target="_blank">Bravyi-Kitaev</a>**: encodes both occupation and parity in a tree structure; reduces the average Z-string length to O(log n). Often preferred for circuits.

**Parity**: reorganizes the encoding around the parity of occupied orbitals; allows two qubits to be tapered off for particle-number-conserving Hamiltonians. Requires the `qiskit` plugin (`import qdk_chemistry.plugins.qiskit`).

The three encodings are unitarily equivalent — they represent the same operator and produce identical spectra. The broader mapping landscape offers further alternatives; for recent work see <a href="https://journals.aps.org/pra/abstract/10.1103/PhysRevA.100.032337" target="_blank">Steudtner & Wehner (2019)</a> and <a href="https://arxiv.org/pdf/2303.02270" target="_blank">arXiv:2303.02270</a>.

**Of these three, Jordan-Wigner is the only encoding supported end-to-end in the QPE pipeline today.** BK and Parity are fully supported for Hamiltonian construction and exact diagonalization — and verified to agree with JW in Part 5 below — but are not yet wired into the time evolution and circuit execution chain.

Now, run the cell to see both mapper settings.