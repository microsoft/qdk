"""Prepare and compare sparse trial states for stretched N2.

The selected-space CASCI wavefunction supplies determinant amplitudes. This
script chooses deterministic leading supports, reoptimizes each support with
projected multi-configuration (PMC), computes ground-state fidelity, synthesizes
the state-preparation circuit, and reports logical gate counts.
"""

# --------------------------------------------------------------------------------------------
# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the MIT License. See LICENSE.txt in the project root for license information.
# --------------------------------------------------------------------------------------------

import json
from collections import Counter
from collections.abc import Iterator
from dataclasses import dataclass

import numpy as np
from qdk_chemistry.algorithms import create
from qdk_chemistry.data import Circuit, Configuration, Hamiltonian, Wavefunction
from qdk_chemistry.data.symmetry import SymmetryLabel, axes
from qdk_chemistry.utils import Logger
from tutorial_choose_active_space import ActiveSpaceResult, run_active_space_workflow


@dataclass
class DeterminantContribution:
    """One determinant's contribution to the selected-space reference.

    Attributes:
        occupation: Spatial-orbital occupation string with storage padding removed.
        amplitude: CASCI coefficient after choosing the physically arbitrary
            global phase so the leading coefficient is positive real.
        weight: Squared coefficient magnitude, ``abs(amplitude)**2``.
        cumulative_weight: Sum of weights through this ranked determinant.
    """

    occupation: str
    amplitude: complex
    weight: float
    cumulative_weight: float


@dataclass
class TrialStateResult:
    r"""Quality and circuit cost for one determinant truncation.

    Fidelity is the squared overlap
    :math:`|\langle\Psi_{\mathrm{trial}}|\Psi_{\mathrm{reference}}\rangle|^2`.

    Attributes:
        num_determinants: Determinants retained by the projected calculation.
        trial_wavefunction: Normalized PMC wavefunction on the retained support.
        fidelity: Squared overlap with the selected-space CASCI ground state.
        circuit: Generated sparse-isometry state-preparation circuit.
        num_compute_qubits: Qubits in the occupation register.
        num_logical_gates: Decomposed leaf operations in the Q# circuit tree.
        logical_gate_counts: Childless-operation counts grouped by displayed gate name.
    """

    num_determinants: int
    trial_wavefunction: Wavefunction
    fidelity: float
    circuit: Circuit
    num_compute_qubits: int
    num_logical_gates: int
    logical_gate_counts: dict[str, int]


@dataclass
class TrialStateWorkflowResult:
    """Reference data and trial-state comparisons used by the chapter.

    Attributes:
        active_space_result: Coordinate-minimized selected molecular model.
        active_hamiltonian: Fermionic Hamiltonian in the selected orbital gauge.
        reference_determinants: Leading CASCI determinants for interpretation.
        trial_states: PMC/circuit results in requested determinant-count order.
    """

    active_space_result: ActiveSpaceResult
    active_hamiltonian: Hamiltonian
    reference_determinants: list[DeterminantContribution]
    trial_states: list[TrialStateResult]


def leading_determinants(
    wavefunction: Wavefunction,
    max_determinants: int,
) -> dict[Configuration, complex]:
    """Select leading determinants with deterministic near-tie ordering.

    Args:
        wavefunction: Reference wavefunction containing aligned determinant and
            coefficient arrays.
        max_determinants: Maximum support size to return.

    Returns:
        An insertion-ordered mapping from selected configurations to their
        unchanged complex coefficients.

    Raises:
        ValueError: If ``max_determinants`` is not positive or exceeds the
            reference support size.
        ValueError: If determinant and coefficient arrays have different lengths.

    Notes:
        Coefficient magnitudes are rounded to twelve decimal places only for
        ranking. This suppresses platform noise near equal weights; the
        occupation string breaks remaining ties. Stored coefficients are never
        rounded or modified.
    """
    if max_determinants <= 0:
        raise ValueError("max_determinants must be positive")
    if max_determinants > wavefunction.size():
        raise ValueError(
            f"requested {max_determinants} determinants from a wavefunction "
            f"with support size {wavefunction.size()}"
        )

    ranked = sorted(
        zip(
            wavefunction.get_active_determinants(),
            wavefunction.get_coefficients(),
            strict=True,
        ),
        key=lambda item: (-round(abs(complex(item[1])), 12), item[0].to_string()),
    )
    return dict(ranked[:max_determinants])


################################################################################
# start-cell-determinant-weights
def leading_determinant_contributions(
    wavefunction: Wavefunction, max_determinants: int = 8
) -> list[DeterminantContribution]:
    """Summarize leading reference determinants and cumulative norm weight.

    Args:
        wavefunction: Selected-space CASCI reference.
        max_determinants: Number of ranked contributions to summarize.

    Returns:
        Ranked determinant records. Occupation strings are truncated to the
        physical active spatial-orbital count because ``Configuration`` storage
        can include trailing zero-valued capacity.
    """
    ranked_determinants = leading_determinants(wavefunction, max_determinants)
    leading_coefficient = complex(next(iter(ranked_determinants.values())))
    global_phase = leading_coefficient.conjugate() / abs(leading_coefficient)
    cumulative_weight = 0.0
    contributions = []
    alpha_channel = SymmetryLabel([axes.alpha()])
    num_active_spatial_orbitals = len(
        wavefunction.get_orbitals().active_indices().indices(alpha_channel)
    )
    for determinant, coefficient in ranked_determinants.items():
        # An eigenvector has arbitrary global phase. Make displayed amplitudes
        # reproducible without changing any weights, fidelities, or circuits.
        amplitude = complex(coefficient) * global_phase

        # Squared amplitudes contribute to the norm; their running sum shows how
        # much of the reference wavefunction the leading determinants capture.
        weight = float(abs(amplitude) ** 2)
        cumulative_weight += weight
        contributions.append(
            DeterminantContribution(
                # Configuration capacity may include zero-valued storage padding;
                # display only the physical selected active spatial orbitals.
                occupation=determinant.to_string()[:num_active_spatial_orbitals],
                amplitude=amplitude,
                weight=weight,
                cumulative_weight=cumulative_weight,
            )
        )
    return contributions
    # end-cell-determinant-weights


################################################################################


################################################################################
# start-cell-circuit-statistics
def iter_decomposed_gate_names(value: object) -> Iterator[str]:
    """Yield normalized names for childless operations in decomposed circuit JSON.

    Args:
        value: A dictionary, list, or scalar from the nested Q# circuit JSON tree.

    Yields:
        Gate names for records without nested child operations. A controlled X is
        displayed as ``CNOT`` for this tutorial's generated circuits.

    Notes:
        The generated circuits use one-control X operations. Production tooling
        should inspect the control count before generalizing this label to
        arbitrary multi-controlled X operations.
    """
    # The circuit is a nested tree of dictionaries and lists. yield from flattens
    # recursive results into one stream of decomposed gate names.
    if isinstance(value, dict):
        children = value.get("children")

        # Composite operations contain children; only gate records without
        # nested child operations are counted.
        if isinstance(children, list) and children:
            for child in children:
                yield from iter_decomposed_gate_names(child)
        else:
            gate_name = value.get("gate")
            if isinstance(gate_name, str):
                # Q# represents CNOT as an X gate with controls; distinguish it
                # from a bare X while the full gate record is available.
                yield (
                    "CNOT" if gate_name == "X" and value.get("controls") else gate_name
                )
        # The children branch was already traversed, so skip that key while
        # checking other fields for additional nested circuit structures.
        for key, child in value.items():
            if key != "children":
                yield from iter_decomposed_gate_names(child)
    elif isinstance(value, list):
        for child in value:
            yield from iter_decomposed_gate_names(child)


def circuit_statistics(circuit: Circuit) -> tuple[int, int, dict[str, int]]:
    """Count qubits and decomposed logical operations in a generated circuit.

    Args:
        circuit: State-preparation circuit whose Q# representation will be
            traversed recursively.

    Returns:
        A tuple containing the qubit count, total decomposed-gate count, and a
        deterministic gate-family count mapping.
    """
    # Convert the Q# circuit JSON to ordinary Python containers, then flatten
    # composite operations to the childless gate records that contribute to the count.
    circuit_data = json.loads(circuit.get_qsharp_circuit().json())
    logical_gate_names = list(iter_decomposed_gate_names(circuit_data))
    logical_gate_counts = Counter(logical_gate_names)
    return (
        len(circuit_data["qubits"]),
        len(logical_gate_names),
        dict(sorted(logical_gate_counts.items())),
    )
    # end-cell-circuit-statistics


################################################################################


def run_trial_state_workflow(
    determinant_counts: tuple[int, ...] = (1, 2, 4),
) -> TrialStateWorkflowResult:
    """Build and compare sparse trial states for the selected N2 Hamiltonian.

    For each requested support size, the largest reference determinants define
    a subspace and PMC reoptimizes amplitudes by diagonalizing the Hamiltonian in
    that subspace. Fidelity is then computed by reading reference and trial
    coefficients in the same PMC determinant order before taking their inner
    product.

    Args:
        determinant_counts: Support sizes to evaluate, in output order.

    Returns:
        The selected molecular model, reference determinant summary, and one
        quality/circuit-cost result per requested support size.

    Raises:
        ValueError: If no support sizes are requested or a requested size is
            invalid for the selected-space reference.
    """
    if not determinant_counts:
        raise ValueError("determinant_counts must not be empty")

    active_space_result = run_active_space_workflow()
    reference_wavefunction = active_space_result.refined_casci_wavefunction
    selected_orbitals = active_space_result.refined_orbitals
    active_hamiltonian = create("hamiltonian_constructor", "qdk").run(selected_orbitals)
    reference_determinants = leading_determinant_contributions(reference_wavefunction)

    trial_states = []
    for num_determinants in determinant_counts:
        ################################################################################
        # start-cell-sparse-trial
        # The leading reference determinants define the trial-state support;
        # PMC then reoptimizes their amplitudes within that restricted space.
        top_determinants = leading_determinants(
            reference_wavefunction, num_determinants
        )
        projected_calculator = create(
            "projected_multi_configuration_calculator", "macis_pmc"
        )
        _, trial_wavefunction = projected_calculator.run(
            active_hamiltonian, list(top_determinants)
        )
        retained_determinants = trial_wavefunction.get_active_determinants()

        # Read both vectors in PMC determinant order. Without this alignment,
        # np.vdot() could multiply coefficients belonging to different determinants.
        reference_coefficients = np.asarray(
            [
                reference_wavefunction.get_coefficient(determinant)
                for determinant in retained_determinants
            ]
        )
        trial_coefficients = np.asarray(
            [
                trial_wavefunction.get_coefficient(determinant)
                for determinant in retained_determinants
            ]
        )

        # The trial vector is normalized on the retained support. The reference
        # entries keep their full-state normalization, so their restricted norm
        # records weight omitted by truncation.
        fidelity = float(abs(np.vdot(reference_coefficients, trial_coefficients)) ** 2)
        # end-cell-sparse-trial
        ################################################################################

        ################################################################################
        # start-cell-preparation-circuit
        state_preparation = create("state_prep", "sparse_isometry_gf2x")
        circuit = state_preparation.run(trial_wavefunction)
        num_compute_qubits, num_logical_gates, logical_gate_counts = circuit_statistics(
            circuit
        )
        # end-cell-preparation-circuit
        ################################################################################

        trial_states.append(
            TrialStateResult(
                num_determinants=trial_wavefunction.size(),
                trial_wavefunction=trial_wavefunction,
                fidelity=fidelity,
                circuit=circuit,
                num_compute_qubits=num_compute_qubits,
                num_logical_gates=num_logical_gates,
                logical_gate_counts=logical_gate_counts,
            )
        )

    return TrialStateWorkflowResult(
        active_space_result=active_space_result,
        active_hamiltonian=active_hamiltonian,
        reference_determinants=reference_determinants,
        trial_states=trial_states,
    )


def print_trial_state_results(result: TrialStateWorkflowResult) -> None:
    """Print determinant, fidelity, and circuit-cost evidence for the lab notebook.

    Args:
        result: Completed trial-state workflow.
    """
    print("Leading selected-space CASCI determinants:")
    print("  Symbols: 2 = doubly occupied, u = alpha, d = beta, 0 = unoccupied")
    print("  Occupation     Amplitude       Weight    Cumulative weight")
    for contribution in result.reference_determinants:
        print(
            f"  {contribution.occupation:<10} "
            f"{contribution.amplitude.real:+.12f}  "
            f"{contribution.weight:.12f}  "
            f"{contribution.cumulative_weight:.12f}"
        )

    print("Sparse trial-state comparison:")
    for trial_state in result.trial_states:
        print(f"\nDeterminants: {trial_state.num_determinants}")
        print(f"Fidelity: {trial_state.fidelity:.12f}")
        print(f"Compute qubits: {trial_state.num_compute_qubits}")
        print(f"Preparation logical gate count: {trial_state.num_logical_gates}")
        print(f"Logical gate-family counts: {trial_state.logical_gate_counts}")


def main() -> None:
    """Run the trial-state workflow and print its lab-notebook evidence."""
    # Change ``off`` to ``info`` to see detailed QDK/Chemistry calculation logs.
    Logger.set_global_level(Logger.LogLevel.off)
    result = run_trial_state_workflow()
    print_trial_state_results(result)


if __name__ == "__main__":
    main()
