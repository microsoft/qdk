# --------------------------------------------------------------------------------------------
# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the MIT License. See LICENSE.txt in the project root for license information.
# --------------------------------------------------------------------------------------------

r"""Iterative QPE demo that swaps Trotterization for exact matrix exponentials.

This script walks through the QDK/Chemistry's SCF → CASCI → IQPE pipeline, in IQPE, it uses Qiskit's
``PauliEvolutionGate`` with a ``MatrixExponential`` synthesizer so each controlled time evolution is exact.
The resulting phase estimate is therefore free of Trotter error, making it a reference point for benchmarking the
Trotterized workflow.
"""

from __future__ import annotations

import numpy as np

try:
    from qiskit import (
        ClassicalRegister,
        QuantumCircuit,
        QuantumRegister,
        transpile,
    )
    from qiskit.circuit.library import PauliEvolutionGate
    from qiskit.quantum_info import SparsePauliOp
    from qiskit.synthesis import MatrixExponential
    from qiskit_aer import AerSimulator
except ImportError as ex:
    raise ImportError(
        "Qiskit and Qiskit-Aer must be installed to run this example. "
        "Please install via 'pip install qiskit qiskit-aer'.",
    ) from ex

from qdk_chemistry.algorithms import create
from qdk_chemistry.data import QpeResult, Structure
from qdk_chemistry.utils import Logger
from qdk_chemistry.utils.phase import (
    iterative_phase_feedback_update,
    phase_fraction_from_feedback,
)

Logger.set_global_level("info")

ACTIVE_ELECTRONS = 2
ACTIVE_ORBITALS = 2
M_PRECISION = 15  # number of phase qubits ~ bits of precision
T_TIME = 0.1  # evolution time; adjust to avoid 2π wrapping for other systems
ITERATION_SHOTS = 10
AER_SIMULATOR = AerSimulator(seed_simulator=42)

# ------------------------------------------------------------------
# Exact-evolution helper routines
# ------------------------------------------------------------------


def create_exact_iteration_circuit(
    state_prep_circuit: QuantumCircuit,
    *,
    iteration: int,
    total_iterations: int,
    phase_correction: float,
    pauli_operator: SparsePauliOp,
    evolution_time: float,
    synthesis: MatrixExponential,
) -> QuantumCircuit:
    """Construct a single IQPE iteration circuit using exact matrix exponentials.

    Args:
        state_prep_circuit: Trial-state preparation circuit.
        iteration: Zero-based iteration index.
        total_iterations: Total number of IQPE iterations.
        phase_correction: Feedback phase applied before evolution.
        pauli_operator: Sparse Pauli operator acting as the Hamiltonian.
        evolution_time: Evolution time ``t`` used for a single application of ``U``.
        synthesis: Matrix exponential synthesizer employed by Qiskit.

    Returns:
        Fully assembled iteration circuit including measurement.

    Notes:
        The library implementation in ``IterativePhaseEstimation`` builds controlled
        time evolutions from precomputed Pauli terms, optimized to avoid Trotterized
        error. For this example we instead hand Qiskit's ``PauliEvolutionGate`` a
        ``SparsePauliOp`` together with a ``MatrixExponential`` synthesizer so that
        each iteration uses an exact matrix exponential. Keeping this helper local
        avoids complicating the shared implementation with synthesis-specific knobs.

    """
    ancilla = QuantumRegister(1, "ancilla")
    system = QuantumRegister(state_prep_circuit.num_qubits, "system")
    classical = ClassicalRegister(1, f"c{iteration}")
    circuit = QuantumCircuit(ancilla, system, classical, name=f"exact_iter_{iteration}")

    circuit.compose(state_prep_circuit, qubits=system, inplace=True)
    circuit.barrier(label="state_prep")

    control = ancilla[0]
    circuit.h(control)
    if not np.isclose(phase_correction, 0.0):
        circuit.rz(phase_correction, control)

    power = 2 ** (total_iterations - iteration - 1)
    scaled_time = evolution_time * power
    evolution_gate = PauliEvolutionGate(pauli_operator, time=scaled_time)
    synthesize_gate = synthesis.synthesize(evolution_gate)
    circuit.append(synthesize_gate.control(1, annotated=True), [control, *system])

    circuit.h(control)
    circuit.measure(control, classical[0])

    return circuit


def run_iterative_exact_qpe(
    state_prep_circuit: QuantumCircuit,
    *,
    pauli_operator: SparsePauliOp,
    precision: int,
    evolution_time: float,
    synthesis: MatrixExponential,
) -> tuple[list[int], float]:
    """Execute iterative QPE with exact evolution and return the bit string and phase.

    Args:
        state_prep_circuit: Trial-state preparation circuit.
        qubit_hamiltonian: Qubit Hamiltonian describing the system.
        pauli_operator: Sparse Pauli operator describing the Hamiltonian.
        precision: Number of IQPE iterations to perform.
        evolution_time: Evolution time ``t`` used for a single application of ``U``.
        synthesis: Matrix exponential synthesizer employed by Qiskit.

    Returns:
        Tuple ``(bits, phase_fraction)`` with the measured bits (MSB to LSB) and
        the resulting phase fraction.

    Notes:
        This routine runs the iterativeqpe with the exact evolution
        primitive using the ``MatrixExponential`` synthesizer for cross-validation
        purposes. Housing it here keeps the matrix-exponential sampling workflow
        alongside the example script instead of pushing Aer-specific plumbing into
        the general ``IterativePhaseEstimation`` module.

    """
    phase_feedback = 0.0
    bits: list[int] = []

    for iteration in range(precision):
        iteration_circuit = create_exact_iteration_circuit(
            state_prep_circuit,
            iteration=iteration,
            total_iterations=precision,
            phase_correction=phase_feedback,
            pauli_operator=pauli_operator,
            evolution_time=evolution_time,
            synthesis=synthesis,
        )
        compiled = transpile(
            iteration_circuit,
            optimization_level=0,
            basis_gates=["h", "x", "y", "z", "rz", "s", "sdg", "cx"],
            seed_transpiler=42,
        )
        result = AER_SIMULATOR.run(compiled, shots=ITERATION_SHOTS).result()
        counts = result.get_counts()
        measured_bit = 0 if counts.get("0", 0) >= counts.get("1", 0) else 1

        bits.append(measured_bit)
        phase_feedback = iterative_phase_feedback_update(phase_feedback, measured_bit)

    phase_fraction = phase_fraction_from_feedback(phase_feedback)
    return bits, phase_fraction


########################################################################################
# 1. QDK/Chemistry calculation for H₂ (0.76 Å, STO-3G)
########################################################################################
structure = Structure(
    np.array([[0.0, 0.0, -0.72], [0.0, 0.0, 0.72]], dtype=float), ["H", "H"]
)  # Geometry in bohr

scf_solver = create("scf_solver")
scf_energy, scf_wavefunction = scf_solver.run(
    structure, charge=0, spin_multiplicity=1, basis_or_guess="sto-3g"
)


########################################################################################
# 2. Find active-space Hamiltonian and CASCI energy
########################################################################################
selector = create(
    "active_space_selector",
    "qdk_valence",
    num_active_electrons=ACTIVE_ELECTRONS,
    num_active_orbitals=ACTIVE_ORBITALS,
)
selected_wavefunction = selector.run(scf_wavefunction)

constructor = create("hamiltonian_constructor")
active_hamiltonian = constructor.run(selected_wavefunction.get_orbitals())

n_alpha = n_beta = ACTIVE_ELECTRONS // 2
mc_calculator = create("multi_configuration_calculator")
casci_energy, casci_wavefunction = mc_calculator.run(
    active_hamiltonian, n_alpha, n_beta
)

core_energy = active_hamiltonian.get_core_energy()

Logger.info("=== Generating QDK/Chemistry artifacts for H2 (0.76 Å, STO-3G) ===")
Logger.info(f"  SCF total energy:   {scf_energy: .8f} Hartree")
Logger.info(f"  CASCI total energy: {casci_energy: .8f} Hartree")


########################################################################################
# 3. Preparing the qubit Hamiltonian and sparse-isometry trial state
########################################################################################
qubit_mapper = create("qubit_mapper", "qiskit", encoding="jordan-wigner")
qubit_hamiltonian = qubit_mapper.run(active_hamiltonian)
qubit_pauli_op = SparsePauliOp(
    qubit_hamiltonian.pauli_strings, qubit_hamiltonian.coefficients
)
num_spin_orbitals = qubit_hamiltonian.num_qubits

top_configurations = casci_wavefunction.get_top_determinants(max_determinants=2)
pmc = create("projected_multi_configuration_calculator")
E_sparse, sparse_wavefunction = pmc.run(
    active_hamiltonian, list(top_configurations.keys())
)

sparse_state_prep = create("state_prep", algorithm_name="sparse_isometry_gf2x")
state_prep = sparse_state_prep.run(sparse_wavefunction).get_qiskit_circuit()
state_prep = transpile(
    state_prep,
    basis_gates=["cx", "rz", "h", "x", "s", "sdg"],
    optimization_level=1,
    seed_transpiler=42,
)
state_prep.name = "casci_sparse_isometry"

Logger.info(
    "\nSparse-isometry state preparation circuit:\n"
    + str(state_prep.draw(output="text"))
)

########################################################################################
# 4. Build and run the Qiskit iterative QPE circuit
########################################################################################
matrix_exp = MatrixExponential()

Logger.info("\n=== Running iterative phase estimation (exact evolution) ===")
Logger.info(f"  Hamiltonian terms: {len(qubit_pauli_op.paulis)}")
Logger.info(f"  System qubits (spin orbitals): {num_spin_orbitals}")
Logger.info(f"  Electron sector (alpha, beta): ({n_alpha}, {n_beta})")

bits, phase_fraction = run_iterative_exact_qpe(
    state_prep,
    pauli_operator=qubit_pauli_op,
    precision=M_PRECISION,
    evolution_time=T_TIME,
    synthesis=matrix_exp,
)


########################################################################################
# 5. Process and display results
########################################################################################
result = QpeResult.from_phase_fraction(
    method="iterative_exact",
    phase_fraction=phase_fraction,
    evolution_time=T_TIME,
    bits_msb_first=bits,
    reference_energy=casci_energy,
)

phase_angle_measured = result.phase_angle
phase_angle_canonical = result.canonical_phase_angle
raw_energy = result.raw_energy
candidate_energies = result.branching
estimated_electronic_energy = (
    result.resolved_energy if result.resolved_energy is not None else raw_energy
)
estimated_total_energy = estimated_electronic_energy + core_energy

Logger.info(f"Measured bits (MSB → LSB): {list(result.bits_msb_first or [])}")
Logger.info(
    f"Phase fraction φ (measured): {result.phase_fraction:.6f} (angle = {phase_angle_measured:.6f} rad)"
)
if not np.isclose(result.phase_fraction, result.canonical_phase_fraction):
    Logger.info(
        f"Canonical phase fraction φ: {result.canonical_phase_fraction:.6f} (angle = {phase_angle_canonical:.6f} rad)",
    )
Logger.info(f"Raw energy_from_phase output: {raw_energy:+.8f} Hartree")
Logger.info("Candidate energies (alias checks):")
for energy in candidate_energies:
    Logger.info(f"  E = {energy:+.8f} Hartree")
Logger.info(f"Estimated electronic energy: {estimated_electronic_energy:.8f} Hartree")
Logger.info(f"Estimated total energy: {estimated_total_energy:.8f} Hartree")
Logger.info(f"Reference total energy (CASCI): {casci_energy:.8f} Hartree")
iterative_energy_error = estimated_total_energy - casci_energy
Logger.info(f"Energy difference (QPE - CASCI): {iterative_energy_error:+.8e} Hartree")
Logger.info(
    "Residual error is now dominated by the finite phase-register resolution because the time evolution is exact."
)
