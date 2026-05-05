# Apply MP2 natural orbital localization
localizer_mp2 = create("orbital_localizer", "qdk_mp2_natural_orbitals")

# run() takes the wavefunction and the active space index bounds
wfn_mp2 = localizer_mp2.run(wfn_valence, *valence_indices)

print("Orbital summary after MP2 natural orbital localization:")
print(wfn_mp2.get_orbitals().get_summary())