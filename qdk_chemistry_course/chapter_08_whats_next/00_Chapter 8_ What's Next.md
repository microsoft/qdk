<h1 style="color:#D30982;text-align:center;">Chapter 8: What's Next</h1>

<h2 style="color:#D30982;">What you built</h2>

You have now completed the full Microsoft QDK Chemistry pipeline — from loading a molecular structure all the way to extracting a ground-state energy via quantum phase estimation on a simulator. Here is the complete arc:

| Chapter | Topic | What you did |
|---|---|---|
| 0 | Getting Started | Set up the QDK, ran a Q# project locally |
| 1 | Molecular Input | Loaded structures, explored `Structure`, `Orbitals`, `Wavefunction` |
| 2 | Classical Functionality | Ran SCF, Pipek-Mezey localization, stability checking |
| 3 | Active Space Selection | Applied valence, autoCAS, AVAS, and autoCAS-EOS selectors |
| 4 | Hamiltonian & Qubit Mapping | Built the molecular Hamiltonian and mapped it to qubits via Jordan-Wigner |
| 5 | State Preparation | Synthesized isometry circuits; compared sparse vs. dense methods |
| 6 | Quantum Phase Estimation | Ran end-to-end IQPE; swept phase bits toward chemical accuracy |
| 7 | Plugins & Extension | Built and registered a custom `ActiveSpaceSelector` using the QDK plugin system |

The system you worked with — the QDK Chemistry library — is designed to scale to real Magne hardware as it becomes available. The skills you developed here are directly transferable to production quantum chemistry workflows.

<h2 style="color:#D30982;">This is a living course</h2>

This course is designed to iterate based on feedback from researchers like you. What you found confusing, what was missing, and what was most useful directly shapes the next version. The sections below outline how to stay involved, file bugs, suggest features, and optionally go deeper with challenge projects.