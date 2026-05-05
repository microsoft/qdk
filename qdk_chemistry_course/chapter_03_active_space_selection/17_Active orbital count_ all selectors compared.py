# Active orbital count: all selectors compared
results = [
    ("qdk_valence (minimal 6e,6o)",    valence_wfns["minimal (6e,6o)"].get_orbitals().get_active_space_indices()[0]),
    ("qdk_valence (default 10e,8o)",   valence_wfns["valence default (10e,8o)"].get_orbitals().get_active_space_indices()[0]),
    ("qdk_valence (expanded 10e,10o)", valence_wfns["expanded (10e,10o)"].get_orbitals().get_active_space_indices()[0]),
    ("pyscf_avas (N 2s+2p)",          avas_indices),
    ("qdk_autocas",                    autocas_indices),
    ("qdk_autocas_eos",                eos_indices),
    ("qdk_occupation",                 occ_indices),
]

print(f"{'Method':<35} {'Orbitals':>9} {'Qubits':>8} {'Indices'}")
print("-" * 75)
for name, indices in results:
    n = len(indices)
    print(f"{name:<35} {n:>9} {2*n:>8}   {list(indices)}")