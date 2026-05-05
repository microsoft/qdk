# Inspect available stability checkers
print("Available stability checkers:", available("stability_checker"))
print()
print_settings("stability_checker", "pyscf")
# Key settings to note:
# - internal: tests stability within same wavefunction type (RHF stays RHF)
# - external: tests RHF → UHF instability (broken-symmetry solutions)
# - stability_tolerance: eigenvalue threshold — negative eigenvalue = unstable direction