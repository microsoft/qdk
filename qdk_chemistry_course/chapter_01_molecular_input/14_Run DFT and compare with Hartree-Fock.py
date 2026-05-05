# Run DFT and compare with Hartree-Fock
scf_dft = create("scf_solver", "pyscf", method="???") 
E_dft, wfn_dft = scf_dft.run(structure, charge=0, spin_multiplicity=1, basis_or_guess="cc-pvdz")

print(f"DFT energy (cc-pvdz): {E_dft:.6f} Hartree")
print(f"HF  energy (cc-pvdz): {E_hf_dz:.6f} Hartree")
print(f"Difference:            {E_dft - E_hf_dz:.6f} Hartree")