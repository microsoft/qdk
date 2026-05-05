# AVAS: atomic-orbital-guided selection
# Specify which atomic orbital character to include — AVAS projects these AOs
# onto the MO basis and selects the orbitals with the largest overlap weight.
avas = create("active_space_selector", "pyscf_avas", ao_labels=["N 2s", "???"])  # fill in: add 2p character for pi bonds
wfn_avas = avas.run(wfn_hf)
avas_indices = wfn_avas.get_orbitals().get_active_space_indices()[0]
print(f"AVAS (2s+2p, sigma+pi): {len(avas_indices)} orbitals → {2*len(avas_indices)} qubits")
print(f"Selected orbital indices: {list(avas_indices)}")

# Unlike qdk_valence (heuristic), AVAS is orbital-character-guided: you specify the atomic
# orbital character that matters for the bonding you want to describe.
# For N₂ dissociation, 2s+2p captures both the sigma and pi bonds.
# AVAS uses the HF wavefunction directly (no SCI step needed).
