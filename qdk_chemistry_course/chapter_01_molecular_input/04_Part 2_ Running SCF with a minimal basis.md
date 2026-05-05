<h2 style="color:#D30982;">Part 2: Running SCF with a minimal basis</h2>

SCF (self-consistent field) is the classical starting point for any quantum chemistry workflow. It produces:
- A **<a href="https://chem.libretexts.org/Bookshelves/Physical_and_Theoretical_Chemistry_Textbook_Maps/Advanced_Theoretical_Chemistry_(Simons)/06:_Electronic_Structure/6.03:_The_Hartree-Fock_Approximation" target="_blank">Hartree-Fock energy</a>** (the best single-determinant approximation to the ground state)
- A <strong>Wavefunction</strong> object containing the molecular orbitals

The `scf_solver` is created via the registry. Its `run()` method takes the structure plus calculation parameters. We will run SCF on stretched N₂ with the `sto-3g` basis. 