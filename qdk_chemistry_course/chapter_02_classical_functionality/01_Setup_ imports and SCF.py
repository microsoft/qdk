# Setup: imports and SCF
from pathlib import Path

import qdk_chemistry.plugins.pyscf

from qdk_chemistry.data import Structure
from qdk_chemistry.algorithms import available, create, print_settings
from qdk_chemistry.utils import Logger, compute_valence_space_parameters

Logger.set_global_level(Logger.LogLevel.off)

N2_XYZ = Path("../examples/data/stretched_n2.structure.xyz")
structure = Structure.from_xyz_file(N2_XYZ)

# Run SCF with cc-pvdz (same as Chapter 1 output)
scf_solver = create("scf_solver")
E_hf, wfn_hf = scf_solver.run(structure, charge=0, spin_multiplicity=1, basis_or_guess="cc-pvdz")
print(f"HF energy: {E_hf:.6f} Hartree")
print("\nCanonical orbital summary:")
print(wfn_hf.get_orbitals().get_summary())