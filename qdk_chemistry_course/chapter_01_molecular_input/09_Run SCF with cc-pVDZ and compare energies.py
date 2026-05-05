# Run SCF with cc-pVDZ and compare energies
scf_solver_dz = create("scf_solver")
E_hf_dz, wfn_dz = scf_solver_dz.run(
    structure,
    charge=0,
    spin_multiplicity=1,
    basis_or_guess="cc-pvdz"
)

print(f"HF energy (sto-3g):  {E_hf_sto3g:.6f} Hartree")
print(f"HF energy (cc-pvdz): {E_hf_dz:.6f} Hartree")
print(f"Basis set effect:    {E_hf_dz - E_hf_sto3g:.6f} Hartree")