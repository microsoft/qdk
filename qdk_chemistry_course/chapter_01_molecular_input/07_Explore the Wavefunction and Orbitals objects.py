# Explore the Wavefunction and Orbitals objects
num_alpha, num_beta = wfn_sto3g.get_total_num_electrons()
print(f"Total electrons: alpha={num_alpha}, beta={num_beta}")

# Orbital summary
orbitals = wfn_sto3g.get_orbitals()
print("\nOrbital summary (sto-3g):")
print(orbitals.get_summary())