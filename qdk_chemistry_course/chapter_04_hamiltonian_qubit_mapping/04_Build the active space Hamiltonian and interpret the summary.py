# Build the active space Hamiltonian and interpret the summary
ham_constructor = create("hamiltonian_constructor")
active_ham = ham_constructor.run(wfn_eos.get_orbitals())
print(active_ham.get_summary())

# Reading the summary:
#   Core Energy      — energy of frozen (non-active) electrons + nuclear repulsion.
#                      This constant is carried into the identity term of the qubit Hamiltonian.
#   One-body Integrals — kinetic + nuclear-attraction matrix elements; n^2 entries for n orbitals.
#   Two-body Integrals — electron-electron repulsion; n^4 entries. This is the dominant scaling term.
#   Active Orbitals  — sets the qubit count: 2 × n_orbitals under Jordan-Wigner or Bravyi-Kitaev.
