# Apply standard autoCAS: entropy-based selection
autocas = create("active_space_selector", "qdk_autocas")
print("autoCAS settings:")
print_settings("active_space_selector", "qdk_autocas")

wfn_autocas = autocas.run(wfn_sci)
autocas_indices = wfn_autocas.get_orbitals().get_active_space_indices()[0]
print(f"\nautoCAS selected {len(autocas_indices)} orbitals: {list(autocas_indices)}")
print(f"Qubit cost: {2 * len(autocas_indices)}")
