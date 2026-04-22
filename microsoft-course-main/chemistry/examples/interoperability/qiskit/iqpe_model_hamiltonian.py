# --------------------------------------------------------------------------------------------
# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the MIT License. See LICENSE.txt in the project root for license information.
# --------------------------------------------------------------------------------------------

r"""Iterative Quantum Phase Estimation demos for non-commuting two-qubit Hamiltonians.

This example demonstrates the use of the native IterativePhaseEstimation algorithm
from QDK/Chemistry to estimate ground state energies of non-commuting Hamiltonians.

Two examples are shown:
1. H = 0.519 * (X ⊗ I) + (Z ⊗ Z) - A simple non-commuting Hamiltonian
2. H = -0.0289(X₁ + X₂) + 0.0541(Z₁ + Z₂) + 0.0150 X₁X₂ + 0.0590 Z₁Z₂ -
   A more complex molecular-inspired Hamiltonian

The IterativePhaseEstimation class handles the QPE protocol, iteratively measuring
phase bits and updating feedback corrections to extract accurate eigenvalue estimates.
"""

import numpy as np

try:
    from qiskit import QuantumCircuit, qasm3
    from qiskit.circuit.library import StatePreparation as QiskitStatePreparation
    from qiskit.quantum_info import SparsePauliOp
except ImportError as ex:
    raise ImportError(
        "Qiskit and Qiskit-Aer must be installed to run this example. "
        "Please install via 'pip install qiskit qiskit-aer'.",
    ) from ex

from qdk_chemistry.algorithms import create
from qdk_chemistry.data import Circuit, QubitHamiltonian
from qdk_chemistry.utils import Logger

Logger.set_global_level("info")

TIME_STEP_1 = np.pi / 4
PHASE_BITS_1 = 6
SHOTS_PER_BIT_1 = 3
SIMULATOR_SEED_1 = 42
REFERENCE_ENERGY_1 = 1.1266592208826944

TIME_STEP_2 = np.pi / 4
PHASE_BITS_2 = 11
SHOTS_PER_BIT_2 = 3
SIMULATOR_SEED_2 = 41
REFERENCE_ENERGY_2 = -0.0887787


########################################################################################
# Example 1: Non-commuting Hamiltonian H = 0.519·XI + 1.0·ZZ
########################################################################################

# 1. Define the Hamiltonian and trial state
hamiltonian_op = SparsePauliOp.from_list(
    [
        ("XI", 0.519),
        ("ZZ", 1.0),
    ]
)
hamiltonian_1 = QubitHamiltonian(
    pauli_strings=hamiltonian_op.paulis.to_labels(),
    coefficients=hamiltonian_op.coeffs,
)

trial_state_1 = np.array([0.97, 0.0, np.sqrt(1 - 0.97**2), 0.0], dtype=complex)
state_prep_1 = QuantumCircuit(2, name="trial")
state_prep_1.append(QiskitStatePreparation(trial_state_1), [0, 1])
state_prep_circuit_1 = Circuit(qasm3.dumps(state_prep_1))

# 2. Run iterative QPE
iqpe_1 = create(
    "phase_estimation",
    "iterative",
    num_bits=PHASE_BITS_1,
    evolution_time=TIME_STEP_1,
    shots_per_bit=SHOTS_PER_BIT_1,
)
simulator_1 = create("circuit_executor", "qiskit_aer_simulator", seed=SIMULATOR_SEED_1)
evolution_builder = create("time_evolution_builder", "trotter")
circuit_mapper = create("controlled_evolution_circuit_mapper", "pauli_sequence")

result_1 = iqpe_1.run(
    state_preparation=state_prep_circuit_1,
    qubit_hamiltonian=hamiltonian_1,
    circuit_executor=simulator_1,
    evolution_builder=evolution_builder,
    circuit_mapper=circuit_mapper,
)

phase_angle_1 = result_1.phase_angle
phase_angle_canonical_1 = result_1.canonical_phase_angle
raw_energy_1 = result_1.raw_energy
candidate_energies_1 = result_1.branching
estimated_energy_1 = (
    result_1.resolved_energy if result_1.resolved_energy is not None else raw_energy_1
)

Logger.info("=== Iterative QPE: Non-commuting Hamiltonian Example ===")
Logger.info("Hamiltonian: H = 0.519 * XI + ZZ")
Logger.info(f"Time step t = pi / 4 ({TIME_STEP_1:.6f}) and {PHASE_BITS_1} phase bits\n")
Logger.info(f"Measured bits (MSB → LSB): {list(result_1.bits_msb_first or [])}")
Logger.info(f"Phase fraction φ (measured): {result_1.phase_fraction:.6f}")
Logger.info(f"Phase angle     (measured) : {phase_angle_1:.6f} rad")
if not np.isclose(result_1.phase_fraction, result_1.canonical_phase_fraction):
    Logger.info(
        f"Canonical phase fraction φ: {result_1.canonical_phase_fraction:.6f} "
        f"(angle = {phase_angle_canonical_1:.6f} rad)",
    )
Logger.info(f"Raw energy_from_phase output: {raw_energy_1:+.8f} Hartree")
Logger.info("Candidate energies (alias checks):")
for energy in candidate_energies_1:
    Logger.info(f"  E = {energy:+.8f} Hartree")
Logger.info(f"Reference energy: {REFERENCE_ENERGY_1:+.8f} Hartree")
Logger.info(f"Estimated energy: {estimated_energy_1:+.8f} Hartree")


########################################################################################
# Example 2: Molecular-inspired non-commuting Hamiltonian
########################################################################################

# 1. Define the Hamiltonian and trial state
hamiltonian_op_2 = SparsePauliOp.from_list(
    [
        ("XI", -0.0289),
        ("IX", -0.0289),
        ("ZI", 0.0541),
        ("IZ", 0.0541),
        ("XX", 0.0150),
        ("ZZ", 0.0590),
    ]
)
hamiltonian_2 = QubitHamiltonian(
    pauli_strings=hamiltonian_op_2.paulis.to_labels(),
    coefficients=hamiltonian_op_2.coeffs,
)

trial_state_2 = np.array([0.0, 0.47, 0.47, 0.75], dtype=complex)
trial_state_2 /= np.linalg.norm(trial_state_2)
state_prep_2 = QuantumCircuit(2, name="trial_2")
state_prep_2.append(QiskitStatePreparation(trial_state_2), [0, 1])
state_prep_circuit_2 = Circuit(qasm3.dumps(state_prep_2))

# 2. Run iterative QPE
iqpe_2 = create(
    "phase_estimation",
    "iterative",
    num_bits=PHASE_BITS_2,
    evolution_time=TIME_STEP_2,
    shots_per_bit=SHOTS_PER_BIT_2,
)
simulator_2 = create("circuit_executor", "qiskit_aer_simulator", seed=SIMULATOR_SEED_2)
evolution_builder = create("time_evolution_builder", "trotter")
circuit_mapper = create("controlled_evolution_circuit_mapper", "pauli_sequence")

result_2 = iqpe_2.run(
    state_preparation=state_prep_circuit_2,
    qubit_hamiltonian=hamiltonian_2,
    circuit_executor=simulator_2,
    evolution_builder=evolution_builder,
    circuit_mapper=circuit_mapper,
)

phase_angle_2 = result_2.phase_angle
phase_angle_canonical_2 = result_2.canonical_phase_angle
raw_energy_2 = result_2.raw_energy
candidate_energies_2 = result_2.branching
estimated_energy_2 = (
    result_2.resolved_energy if result_2.resolved_energy is not None else raw_energy_2
)
Logger.info("\n=== Iterative QPE: Second Non-commuting Hamiltonian Example ===")
Logger.info(
    "Hamiltonian: H = -0.0289(X1 + X2) + 0.0541(Z1 + Z2) + 0.0150 X1X2 + 0.0590 Z1Z2"
)
Logger.info(f"Time step t = pi / 4 ({TIME_STEP_2:.6f}) and {PHASE_BITS_2} phase bits\n")
Logger.info(f"Measured bits (MSB → LSB): {list(result_2.bits_msb_first or [])}")
Logger.info(f"Phase fraction φ (measured): {result_2.phase_fraction:.6f}")
Logger.info(f"Phase angle     (measured) : {phase_angle_2:.6f} rad")
if not np.isclose(result_2.phase_fraction, result_2.canonical_phase_fraction):
    Logger.info(
        f"Canonical phase fraction φ: {result_2.canonical_phase_fraction:.6f} "
        f"(angle = {phase_angle_canonical_2:.6f} rad)",
    )
Logger.info(f"Raw energy_from_phase output: {raw_energy_2:+.8f} Hartree")
Logger.info("Candidate energies (alias checks):")
for energy in candidate_energies_2:
    Logger.info(f"  E = {energy:+.8f} Hartree")
Logger.info(f"Reference energy: {REFERENCE_ENERGY_2:+.8f} Hartree")
Logger.info(f"Estimated energy: {estimated_energy_2:+.8f} Hartree")
