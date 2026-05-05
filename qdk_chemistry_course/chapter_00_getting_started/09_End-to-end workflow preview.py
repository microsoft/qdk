# End-to-end workflow preview
# Each block is labelled with the chapter that covers it in depth.
# Uses a minimal 2e/2o active space and HF trial state for speed.
# Chapter 3 and 5 cover proper active space selection and multi-reference state prep.

from pathlib import Path
import qdk_chemistry.plugins.pyscf
import qdk_chemistry.plugins.qiskit
from qdk_chemistry.data import Structure
from qdk_chemistry.algorithms import create
from qdk_chemistry.utils import Logger

Logger.set_global_level(Logger.LogLevel.off)

# ── Chapter 1: load molecule and run SCF ──────────────────────────────────────
structure = Structure.from_xyz_file(Path("../examples/data/stretched_n2.structure.xyz"))
E_hf, wfn_hf = create("scf_solver").run(
    structure, charge=0, spin_multiplicity=1, basis_or_guess="sto-3g"
)
print(f"[Ch.1] HF energy:            {E_hf:.6f} Hartree")

# ── Chapter 2: localize orbitals ──────────────────────────────────────────────
wfn_valence = create("active_space_selector", "qdk_valence",
                     num_active_electrons=2,
                     num_active_orbitals=2).run(wfn_hf)
indices = wfn_valence.get_orbitals().get_active_space_indices()
wfn_loc = create("orbital_localizer", "qdk_mp2_natural_orbitals").run(wfn_valence, *indices)
print(f"[Ch.2] Orbitals localized")

# ── Chapter 3: active space — 2 electrons in 2 orbitals (HOMO/LUMO) ──────────
n_alpha, n_beta = wfn_loc.get_active_num_electrons()
active_orbitals = wfn_loc.get_orbitals()
print(f"[Ch.3] Active space:          {n_alpha + n_beta} electrons, 2 orbitals → 4 qubits (JW)")

# ── Chapter 4: build Hamiltonian and map to qubits ────────────────────────────
hamiltonian = create("hamiltonian_constructor").run(active_orbitals)
qubit_hamiltonian = create("qubit_mapper", "qiskit", encoding="jordan-wigner").run(hamiltonian)
print(f"[Ch.4] Qubit Hamiltonian:     {len(qubit_hamiltonian.pauli_strings)} Pauli strings")

# ── Chapter 5: prepare trial state from HF wavefunction ──────────────────────
state_prep_circuit = create("state_prep", "sparse_isometry_gf2x").run(wfn_loc)
print(f"[Ch.5] State prep circuit ready")

# ── Chapter 6: iterative QPE ──────────────────────────────────────────────────
evolution_builder = create("time_evolution_builder", "trotter")
circuit_mapper = create("controlled_evolution_circuit_mapper", "pauli_sequence")
iqpe = create("phase_estimation", "iterative", num_bits=2, evolution_time=0.5, shots_per_bit=1)
result = iqpe.run(
    state_preparation=state_prep_circuit,
    qubit_hamiltonian=qubit_hamiltonian,
    circuit_executor=create("circuit_executor", "qdk_full_state_simulator"),
    evolution_builder=evolution_builder,
    circuit_mapper=circuit_mapper,
)
estimated_energy = result.raw_energy + hamiltonian.get_core_energy()
print(f"\n[Ch.6] HF reference energy:   {E_hf:.6f} Hartree")
print(f"[Ch.6] QPE estimated energy:  {estimated_energy:.6f} Hartree")
print(f"[Ch.6] Note: full precision and multi-reference state prep covered in Ch.5-6")
