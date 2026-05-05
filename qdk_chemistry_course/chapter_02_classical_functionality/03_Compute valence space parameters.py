# Compute valence space parameters
num_val_e, num_val_o = compute_valence_space_parameters(wfn_hf, charge=0)
print(f"Valence electrons: {num_val_e}")
print(f"Valence orbitals:  {num_val_o}")

# Select the valence subspace using qdk_valence
valence_selector = create(
    "active_space_selector",
    "qdk_valence",
    num_active_electrons=num_val_e,
    num_active_orbitals=num_val_o
)
wfn_valence = valence_selector.run(wfn_hf)

print("\nValence orbital summary:")
print(wfn_valence.get_orbitals().get_summary())

# Get the active space orbital indices for use in localization
valence_indices = wfn_valence.get_orbitals().get_active_space_indices()
print(f"\nActive space indices: {valence_indices}")