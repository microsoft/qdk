# Apply Foster-Boys localization via PySCF plugin
# After import it registers 'pyscf_multi' as an additional orbital_localizer
print("Available localizers after PySCF import:", available("orbital_localizer"))

# Inspect the pyscf localizer's configurable methods
print()
print_settings("orbital_localizer", "pyscf_multi")

# Apply Foster-Boys via the PySCF localizer
localizer_fb = create("orbital_localizer", "pyscf_multi", method=???) # fill in this blank
wfn_fb = localizer_fb.run(wfn_valence, *valence_indices)

print("\nOrbital summary after Foster-Boys localization:")
print(wfn_fb.get_orbitals().get_summary())

print("\n--- Built-in Pipek-Mezey (for comparison) ---")
print(wfn_pm.get_orbitals().get_summary())