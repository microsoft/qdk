# Setup: SCF, valence space, and MP2 localization
from pathlib import Path

import qdk_chemistry.plugins.pyscf

from qdk_chemistry.data import Structure
from qdk_chemistry.algorithms import available, create, print_settings
from qdk_chemistry.utils import Logger, compute_valence_space_parameters

Logger.set_global_level(Logger.LogLevel.off)

N2_XYZ = Path("../examples/data/stretched_n2.structure.xyz")
structure = Structure.from_xyz_file(N2_XYZ)

# SCF
scf_solver = create("scf_solver")
E_hf, wfn_hf = scf_solver.run(structure, charge=0, spin_multiplicity=1, basis_or_guess="cc-pvdz")
print(f"HF energy: {E_hf:.6f} Hartree")

# Valence space
num_val_e, num_val_o = compute_valence_space_parameters(wfn_hf, charge=0)
valence_selector = create("active_space_selector", "qdk_valence",
                          num_active_electrons=num_val_e, num_active_orbitals=num_val_o)
wfn_valence = valence_selector.run(wfn_hf)
valence_indices = wfn_valence.get_orbitals().get_active_space_indices()

# MP2 natural orbital localization
localizer_mp2 = create("orbital_localizer", "qdk_mp2_natural_orbitals")
wfn_mp2 = localizer_mp2.run(wfn_valence, *valence_indices)
print(f"Valence space: {num_val_e} electrons, {num_val_o} orbitals")
print(f"Starting active space indices: {valence_indices[0]}")
