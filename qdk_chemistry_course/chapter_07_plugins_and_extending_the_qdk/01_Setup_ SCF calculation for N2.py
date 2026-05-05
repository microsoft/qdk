# Setup: SCF calculation for N₂
from pathlib import Path
import numpy as np

import qdk_chemistry.plugins.pyscf
import qdk_chemistry.plugins.qiskit

from qdk_chemistry.data import Structure, Orbitals, Wavefunction, Settings, Configuration
from qdk_chemistry._core.data import SciWavefunctionContainer
from qdk_chemistry.algorithms import available, create, print_settings, register, ActiveSpaceSelector
from qdk_chemistry.algorithms import registry
from qdk_chemistry.utils import Logger, compute_valence_space_parameters

Logger.set_global_level(Logger.LogLevel.off)

N2_XYZ = Path("../examples/data/stretched_n2.structure.xyz")
structure = Structure.from_xyz_file(N2_XYZ)
E_hf, wfn_hf = create("scf_solver").run(structure, charge=0, spin_multiplicity=1, basis_or_guess="cc-pvdz")

n_a, n_b = wfn_hf.get_total_num_electrons()
energies_a, _ = wfn_hf.get_orbitals().get_energies()
print(f"N\u2082: {n_a + n_b} electrons, {wfn_hf.get_orbitals().get_num_molecular_orbitals()} MOs")
print(f"HOMO (index {n_a - 1}): {energies_a[n_a - 1]:.4f} Ha  "
      f"LUMO (index {n_a}): {energies_a[n_a]:.4f} Ha")
print(f"HOMO-LUMO gap: {(energies_a[n_a] - energies_a[n_a - 1]) * 27.211:.3f} eV")