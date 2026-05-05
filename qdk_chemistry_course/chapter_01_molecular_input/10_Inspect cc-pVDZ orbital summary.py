# Inspect cc-pVDZ orbital summary
orbitals_dz = wfn_dz.get_orbitals()
print("Orbital summary (cc-pvdz):")
print(orbitals_dz.get_summary())

# YOUR CODE: How many more orbitals does cc-pvdz produce vs sto-3g?
# print(f"Orbital count increase: {???} orbitals")