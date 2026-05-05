# Print settings table for SCF solver
from qdk_chemistry.algorithms import inspect_settings, print_settings

# Print a formatted settings table for the default SCF solver
print("Settings for scf_solver / qdk:")
print_settings("scf_solver", "qdk")