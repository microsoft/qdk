<h2 style="color:#D30982;">Summary</h2>


**Key patterns:**
```
# Load structure
structure = Structure.from_xyz_file(path)
structure = Structure(coordinates=coords_in_bohr, symbols=["N", "N"])

# SCF
energy, wfn = create("scf_solver").run(structure, charge=0, spin_multiplicity=1, basis_or_guess="cc-pvdz")

# Navigate types correctly
orbitals = wfn.get_orbitals()                             
# Wavefunction → Orbitals
hamiltonian = create("hamiltonian_constructor").run(orbitals)   
# Orbitals → Hamiltonian
total_energy = result.raw_energy + hamiltonian.get_core_energy()      
# always add core energy
```


In this chapter you:
- Mapped the full typed object pipeline: <code>Structure</code> → <code>Wavefunction</code> → <code>Orbitals</code> → <code>Hamiltonian</code> → <code>QubitHamiltonian</code> → <code>Circuit</code> → <code>QpeResult</code>
- Loaded <code>Structure</code> from XYZ file and from coordinates directly — noting that the constructor takes <strong>Bohr</strong>, while XYZ files use Angstrom
- Ran SCF with <code>sto-3g</code> and <code>cc-pvdz</code> and compared energies and orbital counts
- Ran DFT and compared against HF
- Saw how the QDK connects to RDKit, OpenFermion, Qiskit, and PennyLane

The `cc-pvdz` wavefunction (`wfn_dz`) is the starting point for Chapter 2.


You also tested your understanding of various tools and features by answering questions on qBook. 