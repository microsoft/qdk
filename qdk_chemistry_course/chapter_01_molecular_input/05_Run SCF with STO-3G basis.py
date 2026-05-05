# Run SCF with STO-3G basis
scf_solver = create("scf_solver")
E_hf_sto3g, wfn_sto3g = scf_solver.run(
    structure,
    charge=0,
    spin_multiplicity=1,
    basis_or_guess="sto-3g"
)

print(f"HF energy (sto-3g): {E_hf_sto3g:.6f} Hartree")