# Apply Pipek-Mezey localization
localizer_pm = create("orbital_localizer", "qdk_pipek_mezey")
wfn_pm = localizer_pm.run(wfn_valence, *valence_indices)

print("Orbital summary after Pipek-Mezey localization:")
print(wfn_pm.get_orbitals().get_summary())