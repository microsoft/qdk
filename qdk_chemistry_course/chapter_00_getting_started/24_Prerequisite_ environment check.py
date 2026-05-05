# Prerequisite: environment check
from pathlib import Path
import qdk_chemistry
import qdk_chemistry.plugins.pyscf
import qdk_chemistry.plugins.qiskit
from qdk_chemistry.algorithms import available, create

# Check plugins unlocked the expected types
registry = available()
required_types = [
    "scf_solver", "orbital_localizer", "active_space_selector",
    "hamiltonian_constructor", "qubit_mapper", "state_prep",
    "phase_estimation", "circuit_executor",
]
for t in required_types:
    assert t in registry, f"Missing algorithm type: {t}"

# Check N2 data file is present (adjust path if running from a different directory)
n2_path = Path("../examples/data/stretched_n2.structure.xyz")
assert n2_path.exists(), f"N2 structure file not found at {n2_path.resolve()}"

print("All checks passed. Environment is ready — proceed to Chapter 1.")