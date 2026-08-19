"""Estimate the selected-space N2 ground-state energy with native IQPE.

This teaching workflow composes the mapped molecular Hamiltonian, a sparse trial
state, reference-guided phase-grid selection, first-order Trotter evolution, the
native iterative phase-estimation algorithm, and the full-state simulator. The
known CASCI energy is deliberately used to align one finite-bit grid point; this
validates the implementation but is not a production energy-selection strategy.
"""

# --------------------------------------------------------------------------------------------
# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the MIT License. See LICENSE.txt in the project root for license information.
# --------------------------------------------------------------------------------------------

# docs-example: slow

from collections import Counter
from dataclasses import dataclass
from time import perf_counter

import numpy as np
from qdk_chemistry.algorithms import create
from qdk_chemistry.data import AlgorithmRef, Circuit, QubitOperator
from qdk_chemistry.utils import Logger
from tutorial_map_n2_to_qubits import QubitMappingResult, run_qubit_mapping_workflow
from tutorial_prepare_trial_state import TrialStateResult, run_trial_state_workflow

# These teaching defaults balance information content with classical simulator
# cost. Six bits give 64 phase-grid points; three shots provide an odd per-bit
# majority vote; twenty complete runs expose sampling variation; consecutive
# seeds make every simulator result reproducible. One milliHartree is the
# tutorial's algorithmic accuracy target, not a claim of experimental accuracy.
DEFAULT_NUM_PHASE_BITS = 6
DEFAULT_SHOTS_PER_BIT = 3
DEFAULT_NUM_COMPLETE_RUNS = 20
DEFAULT_FIRST_SEED = 42
TARGET_ENERGY_ERROR_HARTREE = 1e-3


@dataclass
class EvolutionTimeChoice:
    r"""Intermediate and final values in reference-guided time selection.

    Attributes:
        bound_time_hartree_inverse: Initial time :math:`\pi/\lambda`, in inverse
            Hartree, whose signed phase interval contains the bounded spectrum.
        bound_reference_phase_fraction: Reference phase fraction at the bound time.
        time_hartree_inverse: Final adjusted evolution time in inverse Hartree.
        reference_phase_fraction: Reference phase fraction at the adjusted time.
        grid_phase_fraction: Selected finite-bit phase-grid fraction.
        grid_bitstring: Most-significant-bit-first representation of that fraction.
        grid_active_energy_hartree: Active-space energy reconstructed from the
            selected grid point, in Hartree.
    """

    bound_time_hartree_inverse: float
    bound_reference_phase_fraction: float
    time_hartree_inverse: float
    reference_phase_fraction: float
    grid_phase_fraction: float
    grid_bitstring: str
    grid_active_energy_hartree: float


@dataclass
class IqpeProblem:
    """Prepared Hamiltonian, trial state, controls, and iteration circuits.

    Attributes:
        mapping: Selected-space fermion-to-qubit mapping result.
        trial_state: Selected trial-state and preparation circuit.
        evolution_time: Reference-guided phase-grid/time selection.
        num_phase_bits: Number of bitwise IQPE iterations.
        shots_per_bit: Simulator executions used for each bit majority vote.
        circuit_builder_reference: Serializable nested algorithm configuration
            supplied to the phase-estimation implementation.
        iteration_circuits: Constructed circuits in controlled-power order; the
            list length equals ``num_phase_bits``.
    """

    mapping: QubitMappingResult
    trial_state: TrialStateResult
    evolution_time: EvolutionTimeChoice
    num_phase_bits: int
    shots_per_bit: int
    circuit_builder_reference: AlgorithmRef
    iteration_circuits: list[Circuit]


@dataclass
class IqpeRun:
    """Measurements, energies, and timing from one complete bitwise IQPE run.

    Attributes:
        seed: Full-state simulator seed.
        bitstring: Canonical most-significant-bit-first phase-grid label.
        phase_fraction: Measured phase fraction in ``[0, 1)``.
        active_energy_hartree: Eigenvalue returned for the qubit Hamiltonian.
        total_energy_hartree: Active energy plus the classically stored core energy.
        error_hartree: Total energy minus the selected-space CASCI reference.
        runtime_seconds: Wall-clock simulator time for this complete six-bit run.
    """

    seed: int
    bitstring: str
    phase_fraction: float
    active_energy_hartree: float
    total_energy_hartree: float
    error_hartree: float
    runtime_seconds: float


@dataclass
class IqpeWorkflowResult:
    """Repeated IQPE results and their unique modal estimate.

    Attributes:
        problem: Shared problem definition used for every repeated run.
        runs: Complete-run results in seed order.
        bitstring_counts: Frequency of each observed complete bitstring.
        modal_run: First run carrying the unique most frequent bitstring.
        total_runtime_seconds: Wall-clock duration of the repeated-run phase.
    """

    problem: IqpeProblem
    runs: list[IqpeRun]
    bitstring_counts: dict[str, int]
    modal_run: IqpeRun
    total_runtime_seconds: float


################################################################################
# start-cell-evolution-time
def choose_reference_guided_evolution_time(
    qubit_hamiltonian: QubitOperator,
    reference_active_energy_hartree: float,
    *,
    num_phase_bits: int = DEFAULT_NUM_PHASE_BITS,
    target_energy_error_hartree: float = TARGET_ENERGY_ERROR_HARTREE,
) -> EvolutionTimeChoice:
    r"""Align a known reference energy with a finite phase-grid point.

    The teaching construction is intentionally reference-guided:

    1. Set :math:`t_{\mathrm{bound}}=\pi/\lambda`, where
       :math:`\lambda=\sum_\ell |h_\ell|` bounds the qubit-Hamiltonian spectrum.
     2. Map the known active-space reference energy to the phase fraction
         :math:`(-Et/2\pi)\bmod 1` at that time.
    3. Round to the nearest point on the :math:`2^m`-point phase grid.
    4. Adjust the evolution time so the chosen grid point reconstructs an energy
       ``target_energy_error_hartree`` above the reference.

    Args:
        qubit_hamiltonian: Mapped Hamiltonian providing ``schatten_norm``
            (the coefficient sum :math:`\lambda`).
        reference_active_energy_hartree: Known selected-space active energy in
            Hartree. This circular input is acceptable only for validation.
        num_phase_bits: Number of binary phase places; the grid has
            :math:`2^{\text{num_phase_bits}}` points.
        target_energy_error_hartree: Signed offset, in Hartree, assigned to the
            selected grid point relative to the reference.

    Returns:
        Every intermediate and final quantity needed to inspect the derivation.

    Raises:
        ValueError: If the precision, Hamiltonian norm, selected phase-grid
            point, target energy, or resulting evolution time is unusable.
    """
    if num_phase_bits <= 0:
        raise ValueError("num_phase_bits must be positive")

    # Start from a time whose signed [-pi, pi] phase interval contains the
    # spectrum bounded by lambda. QubitOperator.schatten_norm is the sum of
    # absolute Pauli coefficients denoted lambda in the chapter derivation.
    coefficient_norm = float(qubit_hamiltonian.schatten_norm)
    if not np.isfinite(coefficient_norm) or coefficient_norm <= 0.0:
        raise ValueError(
            "qubit Hamiltonian coefficient norm must be positive and finite"
        )
    if not np.isfinite(reference_active_energy_hartree):
        raise ValueError("reference active energy must be finite")
    if not np.isfinite(target_energy_error_hartree):
        raise ValueError("target energy error must be finite")

    bound_time = np.pi / coefficient_norm
    reference_phase = (
        -bound_time * reference_active_energy_hartree / (2 * np.pi)
    ) % 1.0
    # Quantize the continuous reference phase to the nearest finite-bit grid
    # point. Modulo handles the wrap from the final grid point back to zero.
    grid_size = 2**num_phase_bits
    grid_index = round(reference_phase * grid_size) % grid_size
    if grid_index == 0:
        raise ValueError(
            "reference energy rounds to the zero-angle phase-grid point; "
            "increase num_phase_bits or choose a different time-selection strategy"
        )
    grid_phase = grid_index / grid_size

    # Convert the grid fraction to the signed angle convention used by the QDK
    # product-formula container. Fractions above one half represent negative
    # angles after subtracting one complete 2*pi turn.
    grid_angle = 2 * np.pi * grid_phase
    if grid_angle > np.pi:
        grid_angle -= 2 * np.pi

    # Solve grid_angle = -t * (E_reference + target_error) for t, matching the
    # container convention E = -angle / t. The measured grid energy therefore
    # differs from the reference by the requested offset.
    target_active_energy = reference_active_energy_hartree + target_energy_error_hartree
    if target_active_energy == 0.0:
        raise ValueError("reference energy plus target error must be nonzero")
    evolution_time = -grid_angle / target_active_energy
    if not np.isfinite(evolution_time) or evolution_time <= 0.0:
        raise ValueError(
            "reference-guided time selection did not produce a positive finite time"
        )
    grid_energy = -grid_angle / evolution_time
    aligned_reference_phase = (
        -evolution_time * reference_active_energy_hartree / (2 * np.pi)
    ) % 1.0
    return EvolutionTimeChoice(
        bound_time_hartree_inverse=float(bound_time),
        bound_reference_phase_fraction=float(reference_phase),
        time_hartree_inverse=float(evolution_time),
        reference_phase_fraction=float(aligned_reference_phase),
        grid_phase_fraction=float(grid_phase),
        grid_bitstring=f"{grid_index:0{num_phase_bits}b}",
        grid_active_energy_hartree=float(grid_energy),
    )
    # end-cell-evolution-time


################################################################################


################################################################################
# start-cell-iqpe-settings
def iqpe_circuit_builder_reference(
    evolution_time_hartree_inverse: float,
    *,
    num_phase_bits: int = DEFAULT_NUM_PHASE_BITS,
) -> AlgorithmRef:
    """Describe native IQPE with repeated first-order Trotter evolution.

    Args:
        evolution_time_hartree_inverse: Base simulated time in inverse Hartree.
        num_phase_bits: Iteration count and number of reported binary phase places.

    Returns:
        A nested algorithm reference selecting the native iterative builder,
        Pauli-sequence controlled-circuit mapping, one first-order Trotter
        division, and repeated base-unitary powers.
    """
    return AlgorithmRef(
        "qpe_circuit_builder",
        "qdk_iterative",
        num_bits=num_phase_bits,
        controlled_circuit_mapper=AlgorithmRef(
            "controlled_circuit_mapper", "pauli_sequence"
        ),
        unitary_builder=AlgorithmRef(
            "hamiltonian_unitary_builder",
            "trotter",
            time=evolution_time_hartree_inverse,
            order=1,
            num_divisions=1,
            power_strategy="repeat",
        ),
    )
    # end-cell-iqpe-settings


################################################################################


def prepare_iqpe_problem(
    *,
    num_phase_bits: int = DEFAULT_NUM_PHASE_BITS,
    shots_per_bit: int = DEFAULT_SHOTS_PER_BIT,
    trial_determinants: int = 4,
) -> IqpeProblem:
    """Prepare all shared inputs and circuits before expensive simulation.

    This function runs the previous mapping and trial-state workflows, selects
    the reference-guided evolution time, records a serializable builder
    reference, and constructs one circuit for each phase bit. Construction is
    separated from execution so students can inspect resources without
    accidentally repeating simulator work.

    Args:
        num_phase_bits: Number of bitwise IQPE iteration circuits.
        shots_per_bit: Simulator shots used by each bit's majority vote.
        trial_determinants: Determinants retained in the selected trial state.

    Returns:
        A complete problem definition ready for ``run_complete_iqpe``.
    """
    mapping = run_qubit_mapping_workflow()
    # Reuse the trial-state support selected by the preceding workflow rather
    # than synthesizing a different state inside the phase-estimation script.
    trial_state = run_trial_state_workflow((trial_determinants,)).trial_states[0]
    evolution_time = choose_reference_guided_evolution_time(
        mapping.qubit_hamiltonian,
        mapping.mapped_active_energy,
        num_phase_bits=num_phase_bits,
    )
    # Keep a serializable reference for the phase-estimation settings and create
    # an instantiated builder separately to inspect all iteration circuits now.
    circuit_builder_ref = iqpe_circuit_builder_reference(
        evolution_time.time_hartree_inverse,
        num_phase_bits=num_phase_bits,
    )
    circuit_builder = create(
        "qpe_circuit_builder",
        "qdk_iterative",
        num_bits=num_phase_bits,
        controlled_circuit_mapper=AlgorithmRef(
            "controlled_circuit_mapper", "pauli_sequence"
        ),
        unitary_builder=AlgorithmRef(
            "hamiltonian_unitary_builder",
            "trotter",
            time=evolution_time.time_hartree_inverse,
            order=1,
            num_divisions=1,
            power_strategy="repeat",
        ),
    )
    iteration_circuits = circuit_builder.run(
        state_preparation=trial_state.circuit,
        qubit_hamiltonian=mapping.qubit_hamiltonian,
    )
    return IqpeProblem(
        mapping=mapping,
        trial_state=trial_state,
        evolution_time=evolution_time,
        num_phase_bits=num_phase_bits,
        shots_per_bit=shots_per_bit,
        circuit_builder_reference=circuit_builder_ref,
        iteration_circuits=iteration_circuits,
    )


################################################################################
# start-cell-run-iqpe
def run_complete_iqpe(problem: IqpeProblem, *, seed: int) -> IqpeRun:
    """Execute one complete bitwise IQPE run with a reproducible simulator seed.

    Args:
        problem: Shared Hamiltonian, trial state, phase controls, and builder setup.
        seed: Full-state simulator seed controlling all finite-shot measurements.

    Returns:
        Canonical bitstring, reconstructed active/total energies, reference error,
        and simulator runtime for one complete phase estimate.
    """
    iqpe = create(
        "phase_estimation",
        "qdk_iterative",
        shots_per_bit=problem.shots_per_bit,
    )
    iqpe.settings().set("qpe_circuit_builder", problem.circuit_builder_reference)
    iqpe.settings().set(
        "circuit_executor",
        AlgorithmRef("circuit_executor", "qdk_full_state_simulator", seed=seed),
    )

    start = perf_counter()
    result = iqpe.run(
        state_preparation=problem.trial_state.circuit,
        qubit_hamiltonian=problem.mapping.qubit_hamiltonian,
    )
    runtime_seconds = perf_counter() - start

    # IQPE measures bits in implementation iteration order. Reconstruct the grid
    # integer from the final phase fraction, then format conventional MSB-first
    # binary so bitstring positions have their familiar place values.
    grid_size = 2**problem.num_phase_bits
    grid_index = round(result.phase_fraction * grid_size) % grid_size
    bitstring = f"{grid_index:0{problem.num_phase_bits}b}"

    # Phase estimation returns the active qubit-Hamiltonian eigenvalue. Add the
    # nuclear/frozen-orbital core term omitted during mapping, then compare with
    # CASCI for exactly the same selected Hamiltonian.
    total_energy = result.raw_energy + problem.mapping.core_energy
    energy_error = total_energy - problem.mapping.active_space_result.refined_energy
    return IqpeRun(
        seed=seed,
        bitstring=bitstring,
        phase_fraction=result.phase_fraction,
        active_energy_hartree=result.raw_energy,
        total_energy_hartree=total_energy,
        error_hartree=energy_error,
        runtime_seconds=runtime_seconds,
    )
    # end-cell-run-iqpe


################################################################################


################################################################################
# start-cell-aggregate-runs
def select_unique_mode(runs: list[IqpeRun]) -> tuple[dict[str, int], IqpeRun]:
    """Select the unique most frequent complete-run bitstring.

    Args:
        runs: Complete IQPE results to aggregate.

    Returns:
        Lexicographically sorted bitstring frequencies and the first run carrying
        the unique modal bitstring.

    Raises:
        ValueError: If no complete runs are supplied.
        RuntimeError: If several bitstrings tie for the highest count. A tie is
            reported rather than hidden because it indicates insufficient
            sampling for one unambiguous tutorial estimate.
    """
    if not runs:
        raise ValueError("at least one complete IQPE run is required")
    bitstring_counts = Counter(run.bitstring for run in runs)
    highest_count = max(bitstring_counts.values())
    modal_bitstrings = sorted(
        bitstring
        for bitstring, count in bitstring_counts.items()
        if count == highest_count
    )
    if len(modal_bitstrings) != 1:
        raise RuntimeError(
            f"complete IQPE runs have no unique mode: {bitstring_counts}"
        )
    modal_bitstring = modal_bitstrings[0]
    modal_run = next(run for run in runs if run.bitstring == modal_bitstring)
    return dict(sorted(bitstring_counts.items())), modal_run
    # end-cell-aggregate-runs


################################################################################


def print_iqpe_settings(
    problem: IqpeProblem,
    *,
    num_complete_runs: int,
    first_seed: int,
) -> None:
    """Print reproducibility settings before the long simulation begins.

    Args:
        problem: Prepared IQPE problem and circuit controls.
        num_complete_runs: Number of complete bitstrings to sample.
        first_seed: First consecutive simulator seed; the final seed follows from
            the run count.
    """
    last_seed = first_seed + num_complete_runs - 1
    print("IQPE settings:")
    print(f"  Trial determinants: {problem.trial_state.num_determinants}")
    print(f"  Trial fidelity: {problem.trial_state.fidelity:.12f}")
    print(f"  Compute qubits: {problem.mapping.num_compute_qubits}")
    print("  Readout ancillas: 1")
    print(f"  Phase bits: {problem.num_phase_bits}")
    print(f"  Shots per bit: {problem.shots_per_bit}")
    print(f"  Complete runs: {num_complete_runs}")
    print(f"  Simulator seeds: {first_seed}-{last_seed}")
    print("  Hamiltonian simulation: first-order Trotter product formula")
    print("  Trotter divisions: 1")
    print("  Controlled powers: repeated approximate base unitary")
    print(
        "  Hamiltonian coefficient sum (lambda): "
        f"{problem.mapping.qubit_hamiltonian.schatten_norm:.12f} Hartree"
    )
    print(
        "  Initial unaliased time bound: "
        f"{problem.evolution_time.bound_time_hartree_inverse:.12f} Hartree^-1"
    )
    print(
        "  Reference phase at initial time bound: "
        f"{problem.evolution_time.bound_reference_phase_fraction:.12f}"
    )
    print(
        "  Selected evolution time: "
        f"{problem.evolution_time.time_hartree_inverse:.12f} Hartree^-1"
    )


def run_iqpe_workflow(
    *,
    num_complete_runs: int = DEFAULT_NUM_COMPLETE_RUNS,
    first_seed: int = DEFAULT_FIRST_SEED,
    num_phase_bits: int = DEFAULT_NUM_PHASE_BITS,
    shots_per_bit: int = DEFAULT_SHOTS_PER_BIT,
    trial_determinants: int = 4,
) -> IqpeWorkflowResult:
    """Prepare and execute repeated complete IQPE runs with progress output.

    Args:
        num_complete_runs: Independent complete bitstrings to sample.
        first_seed: Seed for the first run; later runs use consecutive seeds.
        num_phase_bits: Number of bitwise iteration circuits.
        shots_per_bit: Shots used for each phase-bit majority vote.
        trial_determinants: Determinants retained in the selected trial state.

    Returns:
        Prepared problem, every complete run, bitstring frequencies, unique mode,
        and total repeated-run wall time.

    Raises:
        ValueError: If ``num_complete_runs`` is not positive.
        RuntimeError: If the complete-run bitstrings have no unique mode.
    """
    if num_complete_runs <= 0:
        raise ValueError("num_complete_runs must be positive")
    problem = prepare_iqpe_problem(
        num_phase_bits=num_phase_bits,
        shots_per_bit=shots_per_bit,
        trial_determinants=trial_determinants,
    )
    print_iqpe_settings(
        problem,
        num_complete_runs=num_complete_runs,
        first_seed=first_seed,
    )
    print(
        f"Built {len(problem.iteration_circuits)} IQPE iteration circuits "
        f"for {problem.mapping.num_compute_qubits} compute qubits."
    )
    print(
        f"Reference phase: {problem.evolution_time.reference_phase_fraction:.12f}; "
        f"grid bitstring: {problem.evolution_time.grid_bitstring}"
    )

    workflow_start = perf_counter()
    # Reuse one prepared problem so every run differs only in simulator seed,
    # not Hamiltonian, circuit construction, or trial-state preparation.
    runs = []
    for run_index in range(num_complete_runs):
        seed = first_seed + run_index
        print(
            f"Running complete IQPE {run_index + 1}/{num_complete_runs} "
            f"(seed={seed})...",
            flush=True,
        )
        run = run_complete_iqpe(problem, seed=seed)
        runs.append(run)
        print(
            f"  bits={run.bitstring}, total energy={run.total_energy_hartree:.12f} "
            f"Hartree, error={run.error_hartree:+.3e} Hartree, "
            f"runtime={run.runtime_seconds:.1f} s",
            flush=True,
        )

    total_runtime = perf_counter() - workflow_start
    bitstring_counts, modal_run = select_unique_mode(runs)
    return IqpeWorkflowResult(
        problem=problem,
        runs=runs,
        bitstring_counts=bitstring_counts,
        modal_run=modal_run,
        total_runtime_seconds=total_runtime,
    )


def print_iqpe_results(result: IqpeWorkflowResult) -> None:
    """Print measured values and the final energy comparison for the lab notebook.

    Args:
        result: Completed repeated-IQPE workflow.
    """
    print(f"Complete-run bitstring counts: {result.bitstring_counts}")
    print(f"Modal bitstring: {result.modal_run.bitstring}")
    print(
        f"Modal active-space energy: {result.modal_run.active_energy_hartree:.12f} "
        "Hartree"
    )
    print(f"Core energy: {result.problem.mapping.core_energy:.12f} Hartree")
    print(f"Modal total energy: {result.modal_run.total_energy_hartree:.12f} Hartree")
    print(
        "CASCI algorithmic reference: "
        f"{result.problem.mapping.active_space_result.refined_energy:.12f} Hartree"
    )
    print(f"Modal energy error: {result.modal_run.error_hartree:+.12f} Hartree")
    print(f"Total IQPE runtime: {result.total_runtime_seconds:.1f} seconds")


def main() -> None:
    """Run the complete IQPE workflow and print final lab-notebook evidence."""
    # Change ``off`` to ``info`` to see detailed QDK/Chemistry calculation logs.
    Logger.set_global_level(Logger.LogLevel.off)
    result = run_iqpe_workflow()
    print_iqpe_results(result)


if __name__ == "__main__":
    main()
