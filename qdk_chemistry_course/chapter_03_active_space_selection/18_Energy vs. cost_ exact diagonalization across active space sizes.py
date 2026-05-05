# Energy vs. cost: exact diagonalization across active space sizes
# wfn_mp2, wfn_autocas, wfn_eos, wfn_occ all use the same MP2-localized orbital basis
# so their energies are directly comparable — larger active space = lower (more accurate) energy
ham_constructor = create("hamiltonian_constructor")
qubit_mapper = create("qubit_mapper", "qdk")
solver = create("qubit_hamiltonian_solver", "qdk_sparse_matrix_solver")

energy_cases = [
    ("valence (10e,8o) \u2014 full",  wfn_mp2),
    ("qdk_autocas",                    wfn_autocas),
    ("qdk_autocas_eos",                wfn_eos),
    ("qdk_occupation",                 wfn_occ),
]

print(f"{'Method':<32} {'Qubits':>8} {'Energy (Hartree)':>20}")
print("-" * 64)
for name, wfn in energy_cases:
    orbitals = wfn.get_orbitals()
    n_orb = len(orbitals.get_active_space_indices()[0])
    ham = ham_constructor.run(orbitals)
    qubit_ham = qubit_mapper.run(ham)
    energy, _ = solver.run(qubit_ham)
    print(f"{name:<32} {2*n_orb:>8} {energy:>20.6f}")

# Note: qdk_occupation selected only 2 orbitals for N₂ — an extremely small active
# space. The solver returns E ≈ 0 because it finds the particle-number vacuum sector
# (zero active electrons), not the physical ground state. Very small active spaces can
# produce non-physical diagonalization results; always validate against a larger reference.

print(f"\n{'HF reference':<32} {'\u2014':>8} {E_hf:>20.6f}  (no correlation)")
print(f"{'MACIS ASCI (valence)':<32} {'\u2014':>8} {e_sci:>20.6f}  (multi-ref reference)")
print("\n\u2192 Larger active space \u2192 more qubits, lower energy")
print("\u2192 Carrying forward wfn_eos (autoCAS-EOS) into Chapter 4")
