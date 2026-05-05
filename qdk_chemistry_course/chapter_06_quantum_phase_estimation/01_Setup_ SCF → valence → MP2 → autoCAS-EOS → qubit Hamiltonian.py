# Setup: SCF → valence → MP2 → autoCAS-EOS → qubit Hamiltonian
from pathlib import Path
import numpy as np

import qdk_chemistry.plugins.pyscf
import qdk_chemistry.plugins.qiskit

from qdk_chemistry.data import Structure
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

# Build qubit Hamiltonian (autoCAS-EOS active space, Jordan-Wigner)
active_ham = create("hamiltonian_constructor").run(wfn_eos.get_orbitals())
qham = create("qubit_mapper", "qdk", encoding="jordan-wigner").run(active_ham)

# Exact classical reference via sparse diagonalization
solver = create("qubit_hamiltonian_solver", "qdk_sparse_matrix_solver")
E_exact, _ = solver.run(qham)

# Evolution time: T_max = π / ‖H‖₁
T_max = np.pi / qham.schatten_norm

# State preparation for the EOS wavefunction
circ_eos = create("state_prep", "sparse_isometry_gf2x").run(wfn_eos)
qc_eos = circ_eos.get_qiskit_circuit()

print(f"Qubit Hamiltonian: {qham.num_qubits} qubits, {len(qham.pauli_strings)} Pauli terms")
print(f"Schatten norm: {qham.schatten_norm:.4f}  →  T_max = {T_max:.4f}")
print(f"E_exact (FCI/EOS): {E_exact:.6f} Hartree  ← QPE target")
print(f"circ_eos: {qc_eos.num_qubits} qubits, depth={qc_eos.depth()}")
