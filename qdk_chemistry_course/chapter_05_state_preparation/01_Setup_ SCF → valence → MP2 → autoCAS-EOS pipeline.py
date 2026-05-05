# Setup: SCF → valence → MP2 → autoCAS-EOS pipeline
from pathlib import Path
import numpy as np

import qdk_chemistry.plugins.pyscf
import qdk_chemistry.plugins.qiskit

from qdk_chemistry.data import Structure, Wavefunction
from qdk_chemistry._core.data import SciWavefunctionContainer
from qdk_chemistry.algorithms import available, create, print_settings
from qdk_chemistry.utils import Logger, compute_valence_space_parameters

Logger.set_global_level(Logger.LogLevel.off)

N2_XYZ = Path("../examples/data/stretched_n2.structure.xyz")
structure = Structure.from_xyz_file(N2_XYZ)

E_hf, wfn_hf = create("scf_solver").run(structure, charge=0, spin_multiplicity=1, basis_or_guess="cc-pvdz")
num_val_e, num_val_o = compute_valence_space_parameters(wfn_hf, charge=0)
wfn_val = create("active_space_selector", "qdk_valence",
                 num_active_electrons=num_val_e, num_active_orbitals=num_val_o).run(wfn_hf)
val_idx = wfn_val.get_orbitals().get_active_space_indices()
wfn_mp2 = create("orbital_localizer", "qdk_mp2_natural_orbitals").run(wfn_val, *val_idx)

loc_ham = create("hamiltonian_constructor").run(wfn_mp2.get_orbitals())
macis = create("multi_configuration_calculator", "macis_asci",
               calculate_one_rdm=True, calculate_two_rdm=True)
macis.settings().set("core_selection_strategy", "fixed")
n_a, n_b = wfn_mp2.get_active_num_electrons()
_, wfn_sci = macis.run(loc_ham, n_a, n_b)
wfn_eos = create("active_space_selector", "qdk_autocas_eos").run(wfn_sci)
# wfn_eos is not used in this chapter — it is carried forward to Chapter 6 (QPE setup)

print(f"HF energy: {E_hf:.6f} Hartree")
print(f"SCI wavefunction: {len(wfn_sci.get_active_determinants())} determinants, "
      f"{len(wfn_sci.get_orbitals().get_active_space_indices()[0])} orbitals")
print(f"autoCAS-EOS: {len(wfn_eos.get_active_determinants())} determinant, "
      f"{len(wfn_eos.get_orbitals().get_active_space_indices()[0])} orbitals")