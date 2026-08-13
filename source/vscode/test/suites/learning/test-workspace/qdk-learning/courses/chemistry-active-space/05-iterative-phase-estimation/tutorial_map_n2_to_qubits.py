"""Map the selected stretched-N2 active-space Hamiltonian to qubits.

The workflow constructs the fermionic Hamiltonian from the coordinate-minimized
selected orbitals, applies Jordan--Wigner mapping in blocked spin ordering, and
verifies the mapped ground-state energy in the fixed-electron-number sector.
"""

# --------------------------------------------------------------------------------------------
# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the MIT License. See LICENSE.txt in the project root for license information.
# --------------------------------------------------------------------------------------------

from dataclasses import dataclass

import numpy as np
from qdk_chemistry.algorithms import create
from qdk_chemistry.data import Hamiltonian, MajoranaMapping, QubitOperator
from qdk_chemistry.data.symmetry import SymmetryLabel, axes
from qdk_chemistry.utils import Logger
from tutorial_choose_active_space import (
    ActiveSpaceResult,
    run_active_space_workflow,
)


@dataclass
class QubitMappingResult:
    """Selected-space Hamiltonian and its Jordan--Wigner representation.

    Attributes:
        active_space_result: Molecular model and coordinate-minimized selected orbitals.
        active_hamiltonian: Fermionic Hamiltonian for the selected orbital space.
        qubit_hamiltonian: Weighted Pauli representation of the active Hamiltonian.
        num_active_spatial_orbitals: Selected spatial-orbital count.
        num_active_spin_orbitals: Twice the spatial count for alpha/beta spin.
        num_compute_qubits: Qubits in the mapped occupation register.
        num_pauli_terms: Terms retained by the mapper at its configured threshold.
        core_energy: Nuclear and frozen-inactive contribution, in Hartree, omitted
            from the qubit Hamiltonian and added back classically.
        num_fixed_electron_states: Basis states in the physical alpha/beta sector.
        mapped_active_energy: Lowest mapped eigenvalue in that sector, in Hartree.
        mapped_total_energy: Active eigenvalue plus core energy, in Hartree.
        mapping_energy_difference: Mapped total minus selected-space CASCI energy;
            this should be zero to numerical precision.
    """

    active_space_result: ActiveSpaceResult
    active_hamiltonian: Hamiltonian
    qubit_hamiltonian: QubitOperator
    num_active_spatial_orbitals: int
    num_active_spin_orbitals: int
    num_compute_qubits: int
    num_pauli_terms: int
    core_energy: float
    num_fixed_electron_states: int
    mapped_active_energy: float
    mapped_total_energy: float
    mapping_energy_difference: float


def run_qubit_mapping_workflow() -> QubitMappingResult:
    """Construct, map, and validate the selected stretched-N2 Hamiltonian.

    Returns:
        Fermionic and qubit Hamiltonians, register/term counts, core-energy
        bookkeeping, and an exact fixed-sector mapping check.
    """
    # Reuse the tested active-space workflow so every downstream calculation
    # describes the same selected Hamiltonian without duplicating its construction.
    active_space_result = run_active_space_workflow()

    ################################################################################
    # start-cell-active-hamiltonian
    selected_orbitals = active_space_result.refined_orbitals
    hamiltonian_constructor = create("hamiltonian_constructor", "qdk")
    active_hamiltonian = hamiltonian_constructor.run(selected_orbitals)

    # Nuclear repulsion and frozen inactive-orbital contributions stay outside
    # the mapped active Hamiltonian as one separately stored scalar.
    core_energy = active_hamiltonian.get_core_energy()
    # end-cell-active-hamiltonian
    ################################################################################

    ################################################################################
    # start-cell-count-qubits
    # Count one spin channel to obtain spatial orbitals, then include both spins.
    alpha_channel = SymmetryLabel([axes.alpha()])
    num_active_spatial_orbitals = len(
        selected_orbitals.active_indices().indices(alpha_channel)
    )
    num_active_spin_orbitals = 2 * num_active_spatial_orbitals
    # end-cell-count-qubits
    ################################################################################

    ################################################################################
    # start-cell-map-hamiltonian
    mapping = MajoranaMapping.jordan_wigner(num_modes=num_active_spin_orbitals)
    qubit_mapper = create(
        "qubit_mapper",
        "qdk",
        threshold=1e-10,
        integral_threshold=1e-14,
    )
    qubit_hamiltonian = qubit_mapper.run(active_hamiltonian, mapping)

    # The mapper returns a weighted Pauli sum whose string length is the
    # compute-register size.
    num_compute_qubits = qubit_hamiltonian.num_qubits
    num_pauli_terms = len(qubit_hamiltonian.pauli_strings)
    # end-cell-map-hamiltonian
    ################################################################################

    ################################################################################
    # start-cell-validate-mapping
    num_alpha, num_beta = (
        active_space_result.refined_wavefunction.get_active_num_electrons()
    )

    # QDK/Chemistry's blocked fermion ordering stores alpha occupations in the
    # low bits and beta occupations in the high bits. Shifting 1 left and
    # subtracting 1 creates a mask with one low bit per alpha spin orbital.
    alpha_mask = (1 << num_active_spatial_orbitals) - 1
    # Enumerate all compute-register bit strings, then keep only states with the
    # required numbers of set alpha and beta occupation bits.
    fixed_electron_basis_indices = [
        state
        for state in range(1 << num_compute_qubits)
        if (state & alpha_mask).bit_count() == num_alpha
        and (state >> num_active_spatial_orbitals).bit_count() == num_beta
    ]

    # Construct the full operator sparsely, extract the physical sector, and
    # densify only that compact matrix for exact diagonalization.
    qubit_matrix = qubit_hamiltonian.to_matrix(sparse=True)
    fixed_electron_matrix = qubit_matrix[fixed_electron_basis_indices][
        :, fixed_electron_basis_indices
    ].toarray()
    mapped_active_energy = float(np.linalg.eigvalsh(fixed_electron_matrix)[0])
    mapped_total_energy = core_energy + mapped_active_energy
    mapping_energy_difference = mapped_total_energy - active_space_result.refined_energy
    # end-cell-validate-mapping
    ################################################################################

    return QubitMappingResult(
        active_space_result=active_space_result,
        active_hamiltonian=active_hamiltonian,
        qubit_hamiltonian=qubit_hamiltonian,
        num_active_spatial_orbitals=num_active_spatial_orbitals,
        num_active_spin_orbitals=num_active_spin_orbitals,
        num_compute_qubits=num_compute_qubits,
        num_pauli_terms=num_pauli_terms,
        core_energy=core_energy,
        num_fixed_electron_states=len(fixed_electron_basis_indices),
        mapped_active_energy=mapped_active_energy,
        mapped_total_energy=mapped_total_energy,
        mapping_energy_difference=mapping_energy_difference,
    )


################################################################################
# start-cell-pauli-preview-helpers
def format_pauli_string(pauli_string: str) -> str:
    """Format one QDK Pauli label with explicit qubit indices.

    Args:
        pauli_string: QDK label stored with qubit zero at the right.

    Returns:
        Human-readable factors such as ``X(qubit 1) Z(qubit 3)``. Identity
        factors are omitted; an all-identity label returns ``I``.
    """
    # QubitOperator labels are little-endian: the rightmost character acts on
    # qubit zero. Reverse before enumerate() assigns the displayed qubit index.
    factors = [
        f"{operator}(qubit {qubit_index})"
        for qubit_index, operator in enumerate(reversed(pauli_string))
        if operator != "I"
    ]
    return " ".join(factors) if factors else "I"


def _pauli_term_display_key(term: tuple[str, complex]) -> tuple[float, str]:
    """Rank large Pauli coefficients first with deterministic near-tie order."""
    # Rounding affects only display ranking, not Hamiltonian coefficients. The
    # formatted label resolves coefficients equal at twelve decimal places.
    return (-round(abs(term[1]), 12), format_pauli_string(term[0]))


def representative_pauli_terms(
    qubit_operator: QubitOperator,
    *,
    num_diagonal_terms: int = 3,
    num_off_diagonal_terms: int = 4,
) -> list[tuple[str, complex]]:
    """Select representative identity, diagonal, and coupling terms.

    Args:
        qubit_operator: Mapped weighted Pauli sum.
        num_diagonal_terms: Largest nonidentity I/Z-only terms to retain.
        num_off_diagonal_terms: Largest terms containing X or Y to retain.

    Returns:
        Identity followed by deterministically ranked diagonal and off-diagonal
        ``(Pauli label, coefficient)`` pairs.
    """
    terms = [
        (pauli_string, complex(coefficient))
        for pauli_string, coefficient in zip(
            qubit_operator.pauli_strings,
            qubit_operator.coefficients,
            strict=True,
        )
    ]
    identity_string = "I" * qubit_operator.num_qubits

    # Separate the constant shift, occupation-diagonal terms, and determinant
    # couplings before selecting the largest coefficients in each family.
    identity_terms = [term for term in terms if term[0] == identity_string]
    diagonal_terms = sorted(
        (
            term
            for term in terms
            if term[0] != identity_string and set(term[0]).issubset({"I", "Z"})
        ),
        key=_pauli_term_display_key,
    )
    off_diagonal_terms = sorted(
        (term for term in terms if "X" in term[0] or "Y" in term[0]),
        key=_pauli_term_display_key,
    )
    return (
        identity_terms[:1]
        + diagonal_terms[:num_diagonal_terms]
        + off_diagonal_terms[:num_off_diagonal_terms]
    )
    # end-cell-pauli-preview-helpers


################################################################################


def print_representative_pauli_terms(qubit_operator: QubitOperator) -> None:
    """Print a compact chemical interpretation of mapped Pauli families.

    Args:
        qubit_operator: Mapped Hamiltonian to preview.
    """
    terms = representative_pauli_terms(qubit_operator)
    print(
        f"Representative Pauli terms ({len(terms)} of {len(qubit_operator.pauli_strings)}):"
    )
    identity_string = "I" * qubit_operator.num_qubits
    # Pair each display heading with a predicate so one loop can classify and
    # print all three Pauli-term families consistently.
    categories = (
        ("Constant shift", lambda pauli_string: pauli_string == identity_string),
        (
            "Occupation-sensitive terms (diagonal in the occupation basis)",
            lambda pauli_string: (
                pauli_string != identity_string
                and set(pauli_string).issubset({"I", "Z"})
            ),
        ),
        (
            "Determinant-coupling terms (off-diagonal in the occupation basis)",
            lambda pauli_string: "X" in pauli_string or "Y" in pauli_string,
        ),
    )
    for label, belongs_to_category in categories:
        category_terms = [term for term in terms if belongs_to_category(term[0])]
        if not category_terms:
            continue
        print(f"  {label}:")
        for pauli_string, coefficient in category_terms:
            if abs(coefficient.imag) < 1e-12:
                coefficient_text = f"{coefficient.real:+.12f}"
            else:
                coefficient_text = (
                    f"({coefficient.real:+.12f}{coefficient.imag:+.12f}j)"
                )
            print(f"    {coefficient_text} * {format_pauli_string(pauli_string)}")
    print(f"  ... {len(qubit_operator.pauli_strings) - len(terms)} additional terms")


def print_qubit_mapping_results(result: QubitMappingResult) -> None:
    """Print mapping evidence for the cumulative lab notebook.

    Args:
        result: Completed mapping workflow.
    """
    print(f"Active spatial orbitals: {result.num_active_spatial_orbitals}")
    print(f"Active spin orbitals: {result.num_active_spin_orbitals}")
    print(f"Predicted Jordan-Wigner compute qubits: {result.num_active_spin_orbitals}")
    print(f"Mapped compute qubits: {result.num_compute_qubits}")
    print(f"Pauli terms: {result.num_pauli_terms}")
    print(f"Fermion mode ordering: {result.qubit_hamiltonian.fermion_mode_order}")
    print_representative_pauli_terms(result.qubit_hamiltonian)

    ################################################################################
    # start-cell-core-energy
    num_alpha, num_beta = (
        result.active_space_result.refined_wavefunction.get_active_num_electrons()
    )
    print(
        f"Fixed-electron-number subspace: {num_alpha} alpha, {num_beta} beta electrons "
        f"({result.num_fixed_electron_states} basis states)"
    )
    print(
        f"Core energy stored outside the qubit Hamiltonian: {result.core_energy:.12f} Hartree"
    )
    print(
        f"Mapped active-space ground-state energy: {result.mapped_active_energy:.12f} Hartree"
    )
    print(
        f"Mapped selected-space total energy: {result.mapped_total_energy:.12f} Hartree"
    )
    print(
        f"CASCI algorithmic reference: {result.active_space_result.refined_energy:.12f} Hartree"
    )
    print(
        f"Mapping validation difference: {result.mapping_energy_difference:.3e} Hartree"
    )
    # end-cell-core-energy
    ################################################################################


def main() -> None:
    """Run the mapping workflow and print its lab-notebook evidence."""
    # Change ``off`` to ``info`` to see detailed QDK/Chemistry calculation logs.
    Logger.set_global_level(Logger.LogLevel.off)
    result = run_qubit_mapping_workflow()
    print_qubit_mapping_results(result)


if __name__ == "__main__":
    main()
