<h1 style="color:#D30982;text-align:center;">Chapter 1: Molecular Input & Data Structures</h1>

<h2 style="color:#D30982;">What you'll learn</h2>

- How to load molecule data from file formats and build <code>Structure</code> objects within the QDK
- How to run an SCF and DFT calculation end-to-end
- The QDK's shared vocabulary of typed objects — <code>Structure</code>, <code>Wavefunction</code>, <code>Orbitals</code> — and how they flow between pipeline steps
- What each algorithm consumes and returns — and how to avoid common type confusion errors
- How the Chemistry QDK connects with Qiskit, PennyLane, OpenFermion, and RDKit

<h2 style="color:#D30982;">The molecule for this course</h2>

We use **stretched N₂** throughout — nitrogen with a bond length of 1.27 Å (vs. equilibrium ~1.10 Å). The stretched geometry introduces strong multi-reference character, meaning no single electronic configuration adequately describes the ground state. This makes it an ideal system for exercising the full QDK workflow.