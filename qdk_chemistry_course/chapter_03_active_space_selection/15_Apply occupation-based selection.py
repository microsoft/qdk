# Apply occupation-based selection
occ_selector = create("active_space_selector", "qdk_occupation")
print("Occupation selector settings:")
print_settings("active_space_selector", "qdk_occupation")

wfn_occ = occ_selector.run(wfn_sci)
occ_indices = wfn_occ.get_orbitals().get_active_space_indices()[0]
print(f"\nOccupation-based selection: {len(occ_indices)} orbitals: {list(occ_indices)}")
print(f"Qubit cost: {2 * len(occ_indices)}")
