# Compare active space selectors on N₂: energy_window vs valence
num_val_e, num_val_o = compute_valence_space_parameters(wfn_hf, charge=0)

print(f"{'Selector':<30} {'setting':<22} {'n_active_orbs':>14} {'active_idx'}")
print("-" * 90)

wfn_val = create("active_space_selector", "qdk_valence",
                 num_active_electrons=num_val_e, num_active_orbitals=num_val_o).run(wfn_hf)
ai, _ = wfn_val.get_orbitals().get_active_space_indices()
print(f"  {'qdk_valence':<28} {'(10e / 8o fixed)':<22} {len(ai):>14}  {ai}")

for w in [0.35, 0.5, 1.0]:
    sel = create("active_space_selector", "energy_window")
    sel.settings().set("window_hartree", w)
    wfn_ew = sel.run(wfn_hf)
    ai, _ = wfn_ew.get_orbitals().get_active_space_indices()
    n_ae, _ = wfn_ew.get_active_num_electrons()
    print(f"  {'energy_window':<28} {f'window=±{w} Ha':<22} {len(ai):>14}  {ai}  ({n_ae}α e active)")

print()
print(f"HOMO: {energies_a[n_a-1]:.4f} Ha   LUMO: {energies_a[n_a]:.4f} Ha   "
      f"midpoint: {(energies_a[n_a-1]+energies_a[n_a])/2:.4f} Ha")
print()
print("→ Energy-window selection is a fast heuristic computable from HF alone.")
print("  For strongly correlated systems, occupation-based or entropy-based selectors")
print("  (Chapters 2–3) are more physically targeted; energy-window is useful for")
print("  quick validation or when SCI data is not yet available.")