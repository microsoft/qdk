# Compare Hamiltonians for two active space sizes
ham_constructor = create("hamiltonian_constructor")
valence_ham = ham_constructor.run(wfn_mp2.get_orbitals())   # full valence: 8 orbitals
active_ham  = ham_constructor.run(wfn_eos.get_orbitals())   # autoCAS-EOS:  4 orbitals

print("─── Full valence (8 orbitals, 16 qubits) ───")
print(valence_ham.get_summary())
print("─── autoCAS-EOS (4 orbitals, 8 qubits) ───")
print(active_ham.get_summary())

# Two-body integrals scale as n^4: 8^4 = 4096 vs 4^4 = 256 (16x reduction).
# This directly determines the number of Pauli terms after qubit mapping.
# Halving the active space gives an exponential reduction in circuit complexity downstream.
