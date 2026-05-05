# Create algorithm instances with custom settings
from qdk_chemistry.algorithms import create

# Create the default SCF solver (no name = use the registry default)
scf_default = create("scf_solver")
print("Default SCF solver type:", type(scf_default).__name__)
print("Default settings:", scf_default.settings().to_dict())

print()

# Create a PySCF solver with custom settings passed at creation time
scf_tight = create("scf_solver", "pyscf", max_iterations=100, convergence_threshold=1e-8)
print("Custom SCF solver settings:", scf_tight.settings().to_dict())