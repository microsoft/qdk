# --------------------------------------------------------------------------------------------
# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the MIT License. See LICENSE.txt in the project root for license information.
# --------------------------------------------------------------------------------------------

"""qdk-chemistry + PennyLane quantum phase estimation example.

This example demonstrates the use of PennyLane to implement traditional QFT-based Quantum Phase Estimation (QPE)
using QDK/Chemistry tools for preparing the electronic structure problem.
This example does not use Trotterization; instead, it leverages PennyLane's ability to implement the time-evolution operator `exp(−i*coeff*H)` exactly for a given qubit Hamiltonian.
"""

import numpy as np
from qdk_chemistry.algorithms import create
from qdk_chemistry.data import QpeResult, Structure
from qdk_chemistry.utils import Logger

try:
    import pennylane as qml
except ImportError as e:
    raise ImportError(
        "PennyLane is not installed. Please install PennyLane to run this example: pip install pennylane"
    ) from e

Logger.set_global_level("info")

ACTIVE_ELECTRONS = 2
ACTIVE_ORBITALS = 2
M_PRECISION = 12  # number of phase qubits ~ bits of precision
T_TIME = 0.1  # evolution time; lower if you see 2π wrap


########################################################################################
# 1. QDK/Chemistry calculation for H₂ (1.44 Bohr bond length in STO-3G)
########################################################################################
structure = Structure(
    np.array([[0.0, 0.0, -0.72], [0.0, 0.0, 0.72]], dtype=float), ["H", "H"]
)  # Geometry in bohr

scf_solver = create("scf_solver")  # STO-3G basis for H2
scf_energy, scf_wavefunction = scf_solver.run(
    structure, charge=0, spin_multiplicity=1, basis_or_guess="sto-3g"
)  # Neutral singlet H2


########################################################################################
# 2. Find active space Hamiltonian and CASCI energy
########################################################################################

selector = create(
    "active_space_selector",
    "qdk_valence",
    num_active_electrons=ACTIVE_ELECTRONS,
    num_active_orbitals=ACTIVE_ORBITALS,
)  # Use valence space heuristic
active_orbitals = selector.run(
    scf_wavefunction
).get_orbitals()  # Extract active orbitals

constructor = create("hamiltonian_constructor")
active_hamiltonian = constructor.run(active_orbitals)  # Build active-space Hamiltonian

n_alpha = n_beta = ACTIVE_ELECTRONS // 2
multi_configuration_calculator = create(
    "multi_configuration_calculator"
)  # CASCI solver
casci_energy, _ = multi_configuration_calculator.run(
    active_hamiltonian, n_alpha, n_beta
)  # Solve CASCI

Logger.info("=== Generating QDK/Chemistry artifacts for H2 (0.76 Å, STO-3G) ===")
Logger.info(f"  SCF total energy:   {scf_energy: .4f} Hartree")
Logger.info(f"  CASCI total energy: {casci_energy: .4f} Hartree")


########################################################################################
# 3. Preparing the qubit Hamiltonian for PennyLane QPE
########################################################################################

one_body = np.array(
    active_hamiltonian.get_one_body_integrals(), dtype=float
)  # One-electron integrals
norb = one_body.shape[0]  # Number of spatial orbitals
(two_body_integrals, _, _) = active_hamiltonian.get_two_body_integrals()
two_body_flat = np.array(two_body_integrals, dtype=float)  # Two-electron integrals
two_body = two_body_flat.reshape(
    (norb,) * 4
)  # Make a rank-4 tensor in chemists' notation (pq|rs)
two_body_phys = np.transpose(
    two_body, (0, 2, 1, 3)
)  # Transpose as Pennylane expects physicists' notation <pq|rs>

core_energy = active_hamiltonian.get_core_energy()  # Core energy constant
constant = np.array([core_energy], dtype=float)
fermionic_sentence = qml.qchem.fermionic_observable(
    constant, one_body, two_body_phys
)  # Fermionic Hamiltonian

# Map to qubit Hamiltonian via Jordan-Wigner transformation
H_qubit_raw = qml.jordan_wigner(fermionic_sentence)
num_spin_orbitals = len(H_qubit_raw.wires)
num_spatial_orbitals = num_spin_orbitals // 2

# Offset the qubit Hamiltonian wires by the phase register wires
# 0 -> M_PRECISION
# 1 -> M_PRECISION + 1
# ...
phase_wires = list(range(M_PRECISION))
sys_wires = [w + M_PRECISION for w in range(num_spin_orbitals)]

wire_map: dict = dict(zip(H_qubit_raw.wires, sys_wires, strict=True))
H_qubit = H_qubit_raw.map_wires(wire_map)

Logger.info(f"  Hamiltonian terms: {len(H_qubit)}")  # type: ignore
Logger.info(f"  System qubits (spin orbitals): {num_spin_orbitals}")
Logger.info(f"  Electron sector (alpha, beta): ({n_alpha}, {n_beta})")


########################################################################################
# 4. Build and run the PennyLane QPE circuit
########################################################################################

powers_of_two = [2**i for i in range(len(phase_wires))]

dev = qml.device("default.qubit", wires=phase_wires + sys_wires, shots=None)


@qml.qnode(dev)
def qpe():
    """Run the PennyLane QPE circuit and return phase-register probabilities.

    Returns:
        probs (array[float]): Probabilities of measuring each basis state in the phase register.

    """
    # Prepare phase register in |+> states
    for w in phase_wires:
        qml.Hadamard(wires=w)

    # Prepare system register in Hartree-Fock state
    alpha_wires = sys_wires[:num_spatial_orbitals]
    beta_wires = sys_wires[num_spatial_orbitals : 2 * num_spatial_orbitals]
    for wire in alpha_wires[:n_alpha] + beta_wires[:n_beta]:
        qml.PauliX(wires=wire)

    # Apply controlled time evolutions
    for exponent, ctrl_wire in zip(powers_of_two[::-1], phase_wires, strict=True):
        qml.ctrl(qml.evolve, control=ctrl_wire)(H_qubit, exponent * T_TIME)

    # Inverse QFT on phase register
    qml.adjoint(qml.QFT)(wires=phase_wires)

    return qml.probs(wires=phase_wires)


# Execute the QPE circuit
probs = qpe()
bit_labels = [format(i, f"0{M_PRECISION}b") for i in range(len(probs))]

dominant_index = int(np.argmax(probs))
dominant_bits = bit_labels[dominant_index]
phase_fraction = dominant_index / (2**M_PRECISION)


########################################################################################
# 5. Process and display results
########################################################################################

result = QpeResult.from_phase_fraction(
    method="pennylane_qpe",
    phase_fraction=phase_fraction,
    evolution_time=T_TIME,
    bitstring_msb_first=dominant_bits,
    reference_energy=casci_energy,
)
raw_energy = result.raw_energy
candidate_energies = result.branching
estimated_total_energy = (
    result.resolved_energy if result.resolved_energy is not None else raw_energy
)

Logger.info(f"\nMost likely phase bitstring: {dominant_bits}")
Logger.info(f"Phase fraction φ (measured): {result.phase_fraction:.4f} rad")

Logger.info(f"Estimated total energy: {estimated_total_energy:.4f} Hartree")
Logger.info("Candidate energies (alias checks):")
for energy in candidate_energies:
    Logger.info(f"  E = {energy:.4f} Hartree")

Logger.info(
    f"Total energy difference (QPE - CASCI): {estimated_total_energy - casci_energy:.4e} Hartree"
)
Logger.info(
    "Diagnostic: PennyLane's controlled evolve applies exp(-i H t) exactly, so this residual "
    "difference is dominated by finite phase-register resolution rather than Trotterization."
)
