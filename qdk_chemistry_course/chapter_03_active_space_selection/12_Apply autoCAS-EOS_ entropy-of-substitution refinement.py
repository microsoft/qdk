# Apply autoCAS-EOS: entropy-of-substitution refinement
autocas_eos = create("active_space_selector", "qdk_autocas_eos")
print("autoCAS-EOS settings:")
print_settings("active_space_selector", "qdk_autocas_eos")

wfn_eos = autocas_eos.run(wfn_sci)
eos_indices = wfn_eos.get_orbitals().get_active_space_indices()[0]
print(f"\nautoCAS-EOS selected {len(eos_indices)} orbitals: {list(eos_indices)}")
print(f"Qubit cost: {2 * len(eos_indices)}")

# Compare the two entropy methods
print(f"\n--- Entropy method comparison ---")
print(f"autoCAS:     {list(autocas_indices)}")
print(f"autoCAS-EOS: {list(eos_indices)}")
in_eos_not_autocas = set(eos_indices) - set(autocas_indices)
if in_eos_not_autocas:
    print(f"EOS added orbital(s): {sorted(in_eos_not_autocas)} (captured by substitution entropy, missed by single-orbital entropy)")
else:
    print("Both methods agree on the active space.")
