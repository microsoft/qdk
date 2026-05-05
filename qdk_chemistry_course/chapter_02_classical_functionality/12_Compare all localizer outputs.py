# Compare all localizer outputs
print("=" * 60)
print("CANONICAL (valence)")
print("=" * 60)
print(wfn_valence.get_orbitals().get_summary())

for label, wfn in [("MP2 natural orbitals", wfn_mp2), ("Pipek-Mezey", wfn_pm), ("VVHV", wfn_vvhv)]:
    print("=" * 60)
    print(label)
    print("=" * 60)
    print(wfn.get_orbitals().get_summary())

# What to look for in get_summary():
# - For MP2 natural orbitals: fractional occupations far from 0 or 2 flag strongly correlated orbitals
# - Active space size: a smaller active space with the same chemistry = a cheaper quantum calculation
# - Compare 'Active Orbitals' counts across methods to see how each partitions the orbital space