# Apply VVHV localization
# VVHV differs from other localizers: it requires ALL virtual orbital indices
# (from n_alpha_electrons to num_molecular_orbitals-1), not just the active space.
localizer_vvhv = create("orbital_localizer", "qdk_vvhv")

num_alpha, num_beta = wfn_valence.get_total_num_electrons()
num_mos = wfn_valence.get_orbitals().get_num_molecular_orbitals()
vvhv_indices = (list(range(num_alpha, num_mos)), list(range(num_beta, num_mos)))

wfn_vvhv = localizer_vvhv.run(wfn_valence, *vvhv_indices)

print("Orbital summary after VVHV localization:")
print(wfn_vvhv.get_orbitals().get_summary())