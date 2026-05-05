# Build Hamiltonian and run MACIS ASCI with RDMs
hamiltonian_constructor = create("hamiltonian_constructor")
loc_hamiltonian = hamiltonian_constructor.run(wfn_mp2.get_orbitals())
print("Hamiltonian summary:")
print(loc_hamiltonian.get_summary())

# Run MACIS ASCI with 1-RDM and 2-RDM
# The autoCAS selectors read orbital entanglement entropies from the RDMs.
num_alpha, num_beta = wfn_mp2.get_active_num_electrons()
print(f"\nActive electrons: \u03b1={num_alpha}, \u03b2={num_beta}")

macis = create("multi_configuration_calculator", "macis_asci",
               calculate_one_rdm=True,
               calculate_two_rdm=True)
macis.settings().set("core_selection_strategy", "fixed")  # required when starting from a single HF determinant

e_sci, wfn_sci = macis.run(loc_hamiltonian, num_alpha, num_beta)
print(f"\nMACIS ASCI energy: {e_sci:.6f} Hartree")
print(f"Number of determinants: {len(wfn_sci.get_active_determinants())}")