"""Utility functions for QPE."""

# --------------------------------------------------------------------------------------------
# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the MIT License. See LICENSE.txt in the project root for license information.
# --------------------------------------------------------------------------------------------

import numpy as np
from qdk_chemistry.algorithms import create
from qdk_chemistry.data import Hamiltonian, QubitHamiltonian, Wavefunction


def prepare_top_dets_trial_state(
    wf: Wavefunction, hamiltonian: Hamiltonian, num_dets: int
) -> tuple[Wavefunction, float]:
    """Prepare a trial state for QPE using the top determinants from the given wavefunction.

    Args:
        wf: Original wavefunction used to extract the top determinants.
        hamiltonian: Hamiltonian used to compute the projected multi-configuration state.
        num_dets: Number of top determinants to use.

    Returns:
        wavefunction: Wavefunction object built from the top determinants.
        fidelity: Fidelity with respect to the original wavefunction (overlap squared).

    """
    dets = wf.get_top_determinants(max_determinants=num_dets)
    if not dets:
        raise ValueError(
            "Cannot prepare trial state: No determinants found in the wavefunction."
        )

    pmc_calculator = create("projected_multi_configuration_calculator", "macis_pmc")
    _, wf_trial = pmc_calculator.run(hamiltonian, list(dets.keys()))

    # Fidelity with original reference wf
    det_keys = list(dets.keys())
    coeffs_wf = np.array([dets[det] for det in det_keys])
    coeffs_new = np.array([wf_trial.get_coefficient(det) for det in det_keys])
    fidelity = np.abs(np.vdot(coeffs_new, coeffs_wf)) ** 2

    return wf_trial, fidelity


def compute_evolution_time(
    qubit_hamiltonian: QubitHamiltonian,
    num_bits: int,
    solve_hamiltonian: bool = True,
    target_energy_precision: float = 1e-3,
    initial_time_guess: float | None = None,
) -> float:
    """Compute an evolution time for iterative QPE to achieve a target energy precision.

    Starts from a base time of pi / ||H|| (Schatten norm of qubit Hamiltonian), or an optional initial_time_guess.
    If solve_hamiltonian is True, obtains a reference energy from qubit Hamiltonian via a sparse matrix solver
    and refines the time to reduce phase discretization error given num_bits of precision.
    If False, returns the base time without refinement.

    Args:
        qubit_hamiltonian: Qubit Hamiltonian.
        num_bits: Number of precision bits used in iterative QPE.
        solve_hamiltonian: Whether to solve for a reference energy to refine the time. Defaults to True.
        target_energy_precision: Desired energy precision in Hartree. Defaults to 1e-3.
        initial_time_guess: Optional initial evolution time guess.

    Returns:
        Computed evolution time.

    """
    # Compute base evolution time from Hamiltonian norm or use provided guess
    bound_time = np.pi / qubit_hamiltonian.schatten_norm
    base_time = (
        min(initial_time_guess, bound_time)
        if initial_time_guess is not None
        else bound_time
    )

    if not solve_hamiltonian:
        return base_time

    # Use the reference energy from the qubit Hamiltonian
    solver = create("qubit_hamiltonian_solver", "qdk_sparse_matrix_solver")
    reference_energy, _ = solver.run(qubit_hamiltonian)

    # Compute the expected phase from the reference energy
    expected_phase = (base_time * reference_energy) / (2 * np.pi) % 1

    # Discretize to the nearest representable phase with given precision bits
    bit_phase = round(expected_phase * 2**num_bits) / 2**num_bits

    # Compute the energy error from phase discretization
    if abs(base_time) < np.finfo(np.float64).eps:
        raise ValueError(
            f"Cannot compute discretization energy error: base_time {base_time} is too close to zero."
        )
    discretization_energy_error = (2 * np.pi * (bit_phase - expected_phase)) / base_time

    # Shift the energy error to achieve the target precision
    shifted_energy = discretization_energy_error - target_energy_precision

    # Compute the adjusted evolution time, guarding against division by zero
    denominator = reference_energy + target_energy_precision
    if abs(denominator) < np.finfo(np.float64).eps:
        raise ValueError(
            "Cannot compute adjusted evolution time: reference_energy + "
            f"target_energy_precision is too close to zero "
            f"(reference_energy={reference_energy}, "
            f"target_energy_precision={target_energy_precision})."
        )

    proposed_time = base_time + shifted_energy * base_time / denominator
    return proposed_time
