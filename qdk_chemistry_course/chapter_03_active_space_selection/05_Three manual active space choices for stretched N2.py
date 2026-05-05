# Three manual active space choices for stretched N₂
choices = [(6, 6), (num_val_e, num_val_o), (10, 10)]
labels  = ["minimal (6e,6o)", "valence default (10e,8o)", "expanded (10e,10o)"]

valence_wfns = {}
for (ne, no), label in zip(choices, labels):
    sel = create("active_space_selector", "qdk_valence",
                 num_active_electrons=ne, num_active_orbitals=no)
    wfn = sel.run(wfn_hf)
    valence_wfns[label] = wfn
    active_o = wfn.get_orbitals().get_active_space_indices()[0]
    n_qubits = 2 * len(active_o)
    print(f"{label}: {len(active_o)} active orbitals → {n_qubits} qubits")
    print(f"  indices: {list(active_o)}")
