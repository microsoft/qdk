"""Choose reproducible coordinates inside degenerate orbital subspaces.

Natural orbitals with equal occupations define a subspace but not unique
orbital vectors. This module anchors those subspaces to the atomic-orbital
basis, then coordinate-minimizes the mapped Hamiltonian coefficient norm while
preserving the selected orbital subspace and its exact CASCI energy.
"""

# --------------------------------------------------------------------------------------------
# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the MIT License. See LICENSE.txt in the project root for license information.
# --------------------------------------------------------------------------------------------

from collections.abc import Callable
from dataclasses import dataclass, field
from itertools import combinations
from typing import Any

import numpy as np
from qdk_chemistry.algorithms import create
from qdk_chemistry.data import MajoranaMapping, Orbitals, Wavefunction
from qdk_chemistry.data.symmetry import SymmetryLabel, axes


@dataclass(frozen=True)
class NaturalOrbitalCoordinateMinimizationResult:
    """Orbitals and diagnostics from deterministic coordinate minimization.

    Here *gauge* means a choice of orbital vectors used as coordinates for a
    fixed occupation-degenerate subspace. Different gauges span the same
    subspace and have the same exact CASCI energy, but can produce different
    mapped Hamiltonian representations.

    Attributes:
        orbitals: Selected orbitals after AO anchoring and coordinate minimization.
        selected_blocks: Natural-orbital index blocks present in the selected
            active space. Singleton blocks require no rotational search.
        coefficient_norm_before: Mapped coefficient norm ``lambda`` in Hartree
            after AO anchoring and before coordinate rotations.
        coefficient_norm_after: Mapped coefficient norm in Hartree after accepted
            coordinate rotations.
        effective_pauli_terms_before: Pauli terms above the diagnostic threshold
            after AO anchoring and before coordinate rotations.
        effective_pauli_terms_after: Corresponding effective term count after
            gauge selection; this is reported but not optimized directly.
    """

    orbitals: Orbitals
    selected_blocks: tuple[tuple[int, ...], ...]
    coefficient_norm_before: float
    coefficient_norm_after: float
    effective_pauli_terms_before: int
    effective_pauli_terms_after: int


def _natural_occupation_blocks(
    reference_wavefunction: Wavefunction,
    natural_indices: list[int],
    *,
    degeneracy_tolerance: float = 1e-6,
) -> tuple[tuple[int, ...], ...]:
    """Group consecutive sorted natural occupations into degeneracy blocks.

    Args:
        reference_wavefunction: Correlated wavefunction containing the active
            spin-traced one-particle RDM.
        natural_indices: Orbital indices ordered like the descending natural
            occupations.
        degeneracy_tolerance: Maximum occupation-number gap within one block.

    Returns:
        Non-overlapping ordered blocks that cover ``natural_indices``.
    """
    one_rdm = np.asarray(reference_wavefunction.get_active_one_rdm_spin_traced())
    occupations = np.linalg.eigvalsh(0.5 * (one_rdm + one_rdm.T))[::-1]

    occupation_blocks: list[tuple[int, ...]] = []
    block_start = 0
    for block_stop in range(1, len(occupations) + 1):
        if block_stop == len(occupations) or (
            abs(occupations[block_stop] - occupations[block_stop - 1])
            >= degeneracy_tolerance
        ):
            occupation_blocks.append(tuple(natural_indices[block_start:block_stop]))
            block_start = block_stop
    return tuple(occupation_blocks)


def _orbitals_with_coefficients(
    template: Orbitals, coefficients: np.ndarray
) -> Orbitals:
    """Copy an orbital object while replacing its coefficient matrix.

    Args:
        template: Source of overlap, basis-set, and active/inactive metadata.
        coefficients: Replacement AO-by-MO coefficient matrix.

    Returns:
        Restricted orbitals with replacement coefficients and metadata copied
        from ``template``. Orbital energies are omitted because rotations inside
        a correlated active space do not assign unique one-electron energies.
    """
    return Orbitals(
        coefficients,
        None,
        template.get_overlap_matrix(),
        template.get_basis_set(),
        template.active_indices(),
        template.inactive_indices(),
    )


def _ao_anchor_block(block: np.ndarray, overlap: np.ndarray) -> np.ndarray:
    """Orient one degenerate block reproducibly using atomic-orbital anchors.

    Args:
        block: AO-by-orbital coefficient matrix for one degenerate block.
        overlap: Atomic-orbital overlap matrix.

    Returns:
        Coefficients spanning the same subspace in deterministic coordinates.

    Raises:
        RuntimeError: If independent AO anchors cannot be found.
    """
    projected_ao = overlap @ block
    residuals = projected_ao.copy()
    anchors = []

    for _ in range(block.shape[1]):
        projection_norms = np.einsum("ij,ij->i", residuals, residuals)
        anchor = int(np.argmax(np.round(projection_norms, decimals=14)))
        anchors.append(anchor)
        anchor_vector = residuals[anchor].copy()
        anchor_norm = np.linalg.norm(anchor_vector)
        if anchor_norm <= np.finfo(float).eps:
            raise RuntimeError(
                "Unable to find independent AO anchors for a natural-orbital block"
            )
        anchor_vector /= anchor_norm
        residuals -= np.outer(residuals @ anchor_vector, anchor_vector)

    anchor_coefficients = projected_ao[anchors].T
    gram_values, gram_vectors = np.linalg.eigh(
        anchor_coefficients.T @ anchor_coefficients
    )
    if np.min(gram_values) <= np.finfo(float).eps:
        raise RuntimeError("Natural-orbital AO anchors are linearly dependent")
    orthogonalizer = (
        anchor_coefficients @ gram_vectors @ np.diag(gram_values**-0.5) @ gram_vectors.T
    )
    return block @ orthogonalizer


def _givens_rotation(size: int, left: int, right: int, angle: float) -> np.ndarray:
    """Construct an orthogonal plane rotation inside an orbital block.

    Args:
        size: Dimension of the degenerate orbital block.
        left: First zero-based block-column index to rotate.
        right: Second zero-based block-column index to rotate.
        angle: Rotation angle in radians.

    Returns:
        A ``size``-by-``size`` identity matrix with the selected plane replaced
        by a Givens rotation.
    """
    rotation = np.eye(size)
    cosine = np.cos(angle)
    sine = np.sin(angle)
    rotation[left, left] = cosine
    rotation[left, right] = -sine
    rotation[right, left] = sine
    rotation[right, right] = cosine
    return rotation


def _golden_section_minimum(
    objective: Callable[[float], float],
    lower_bound: float,
    upper_bound: float,
    *,
    argument_tolerance: float = 1e-13,
) -> tuple[float, float]:
    """Refine a bracketed scalar minimum without a machine-epsilon floor.

    The mapped coefficient norm is a sum of absolute values and can have a cusp
    where symmetry-related Pauli coefficients vanish. Common bounded minimizers
    include a machine-epsilon-dependent stopping term that can leave
    platform-dependent residual coefficients near such cusps. Golden-section
    contraction uses the requested absolute angular tolerance directly.

    Args:
        objective: Scalar function to minimize.
        lower_bound: Inclusive lower end of the bracketing interval.
        upper_bound: Inclusive upper end of the bracketing interval.
        argument_tolerance: Maximum final interval width.

    Returns:
        The best sampled argument and its objective value.

    Raises:
        ValueError: If the interval or tolerance is not positive.
    """
    if upper_bound <= lower_bound:
        raise ValueError("upper_bound must be greater than lower_bound")
    if argument_tolerance <= 0.0:
        raise ValueError("argument_tolerance must be positive")

    inverse_golden_ratio = (np.sqrt(5.0) - 1.0) / 2.0
    left = float(lower_bound)
    right = float(upper_bound)
    inner_left = right - inverse_golden_ratio * (right - left)
    inner_right = left + inverse_golden_ratio * (right - left)
    value_left = float(objective(inner_left))
    value_right = float(objective(inner_right))

    while right - left > argument_tolerance:
        if value_left <= value_right:
            right = inner_right
            inner_right = inner_left
            value_right = value_left
            inner_left = right - inverse_golden_ratio * (right - left)
            value_left = float(objective(inner_left))
        else:
            left = inner_left
            inner_left = inner_right
            value_left = value_right
            inner_right = left + inverse_golden_ratio * (right - left)
            value_right = float(objective(inner_right))

    midpoint = (left + right) / 2.0
    candidates = (
        (inner_left, value_left),
        (inner_right, value_right),
        (midpoint, float(objective(midpoint))),
    )
    return min(candidates, key=lambda candidate: (candidate[1], candidate[0]))


@dataclass
class MappedHamiltonianEvaluator:
    """Evaluate mapped-Hamiltonian metrics for candidate orbital coordinates.

    Attributes:
        orbital_template: Source of basis and active-space metadata.
        num_selected_orbitals: Number of selected spatial orbitals.
        effective_pauli_threshold: Coefficient threshold used for diagnostics.
        hamiltonian_constructor: Reusable fermionic Hamiltonian constructor.
        qubit_mapper: Reusable mapper configured with tight search thresholds.
        mapping: Jordan--Wigner mapping for the selected spin-orbital count.
    """

    orbital_template: Orbitals
    num_selected_orbitals: int
    effective_pauli_threshold: float
    hamiltonian_constructor: Any = field(init=False, repr=False)
    qubit_mapper: Any = field(init=False, repr=False)
    mapping: MajoranaMapping = field(init=False, repr=False)

    def __post_init__(self) -> None:
        """Construct reusable Hamiltonian and mapping algorithms."""
        self.hamiltonian_constructor = create("hamiltonian_constructor", "qdk")
        self.qubit_mapper = create(
            "qubit_mapper",
            "qdk",
            threshold=1e-14,
            integral_threshold=1e-14,
        )
        self.mapping = MajoranaMapping.jordan_wigner(
            num_modes=2 * self.num_selected_orbitals
        )

    def evaluate(self, coefficients: np.ndarray) -> tuple[float, int]:
        """Return coefficient norm and effective Pauli count for coordinates.

        The mapper retains coefficients down to ``1e-14`` during optimization
        so pruning does not create artificial minima. The separately reported
        count uses ``effective_pauli_threshold`` as a stable diagnostic.

        Args:
            coefficients: Candidate AO-by-MO coefficient matrix.

        Returns:
            Mapped coefficient norm in Hartree and the number of Pauli terms at
            or above the diagnostic threshold.
        """
        candidate_orbitals = _orbitals_with_coefficients(
            self.orbital_template, coefficients
        )
        hamiltonian = self.hamiltonian_constructor.run(candidate_orbitals)
        qubit_hamiltonian = self.qubit_mapper.run(hamiltonian, self.mapping)
        effective_terms = int(
            np.count_nonzero(
                np.abs(qubit_hamiltonian.coefficients) >= self.effective_pauli_threshold
            )
        )
        return float(qubit_hamiltonian.schatten_norm), effective_terms


@dataclass
class PlaneRotationObjective:
    """One-dimensional coefficient-norm objective for a Givens rotation.

    Attributes:
        evaluator: Shared mapped-Hamiltonian metric evaluator.
        base_coefficients: Full coefficient matrix before this coordinate step.
        block_coefficients: Snapshot of the block before this coordinate step.
        block: Global orbital indices in the degenerate block.
        left: First block-local column participating in the rotation.
        right: Second block-local column participating in the rotation.
    """

    evaluator: MappedHamiltonianEvaluator
    base_coefficients: np.ndarray
    block_coefficients: np.ndarray
    block: tuple[int, ...]
    left: int
    right: int

    def __call__(self, angle: float) -> float:
        """Return the mapped coefficient norm at one rotation angle."""
        candidate_coefficients = self.base_coefficients.copy()
        candidate_coefficients[:, self.block] = (
            self.block_coefficients
            @ _givens_rotation(len(self.block), self.left, self.right, angle)
        )
        return self.evaluator.evaluate(candidate_coefficients)[0]


def coordinate_minimize_natural_orbital_coefficient_norm(
    reference_wavefunction: Wavefunction,
    selected_orbitals: Orbitals,
    natural_indices: list[int],
    *,
    degeneracy_tolerance: float = 1e-6,
    angle_samples: int = 32,
    max_sweeps: int = 3,
    improvement_tolerance: float = 1e-10,
    effective_pauli_threshold: float = 1e-10,
) -> NaturalOrbitalCoordinateMinimizationResult:
    r"""Coordinate-minimize :math:`\lambda` inside degenerate orbital blocks.

    Natural orbitals with equal occupations define a subspace but not unique
    orbital vectors. AO anchoring first gives every selected occupation block
    deterministic coordinates. Deterministic coordinate sweeps then search
    plane rotations within degenerate blocks and accept only reductions in the
    mapped coefficient norm :math:`\lambda=\sum_\ell |h_\ell|`.

    Rotations stay inside occupation-degenerate blocks, so they preserve the
    selected orbital subspace and its exact CASCI energy. Effective Pauli counts
    are diagnostics rather than the optimization objective because raw counts
    are sensitive to pruning noise.

    The search is coordinate descent, not a global-minimum proof for arbitrary
    high-dimensional blocks. Each sweep visits every Givens plane in a fixed
    order. A coarse angular grid identifies a candidate basin, and
    golden-section contraction refines it without a machine-epsilon stopping
    floor.

    Args:
        reference_wavefunction: Correlated valence-space wavefunction whose
            spin-traced one-particle RDM defines occupation degeneracies.
        selected_orbitals: autoCAS-selected natural orbitals to canonicalize.
        natural_indices: Valence orbital indices ordered like the natural
            occupations in ``reference_wavefunction``.
        degeneracy_tolerance: Occupation-number gap below which consecutive
            natural orbitals belong to one degenerate block.
        angle_samples: Uniform samples over :math:`[0,\pi)` used to locate a
            candidate basin for each plane rotation.
        max_sweeps: Maximum deterministic passes over all coordinate planes.
        improvement_tolerance: Minimum reduction in :math:`\lambda`, in Hartree,
            required to accept a rotation or begin another sweep.
        effective_pauli_threshold: Absolute Pauli-coefficient threshold, in
            Hartree, used only for before/after diagnostic counts.

    Returns:
        Coordinate-minimized orbitals and before/after mapping diagnostics.

    Raises:
        ValueError: If fewer than four angular samples are requested.
        RuntimeError: If autoCAS splits an occupation-degenerate block or AO
            anchoring cannot construct deterministic coordinates.
    """
    if angle_samples < 4:
        raise ValueError("angle_samples must be at least four")

    alpha_channel = SymmetryLabel([axes.alpha()])
    selected_indices = tuple(selected_orbitals.active_indices().indices(alpha_channel))
    selected_index_set = set(selected_indices)
    selected_blocks = []
    for block in _natural_occupation_blocks(
        reference_wavefunction,
        natural_indices,
        degeneracy_tolerance=degeneracy_tolerance,
    ):
        selected_members = selected_index_set.intersection(block)
        if selected_members and len(selected_members) != len(block):
            raise RuntimeError(
                f"The selected active space splits degenerate natural-orbital block {block}"
            )
        if selected_members:
            selected_blocks.append(block)

    coefficients = np.array(
        selected_orbitals.coefficients().block((alpha_channel, alpha_channel)),
        copy=True,
    )
    overlap = selected_orbitals.get_overlap_matrix()
    for block in selected_blocks:
        coefficients[:, block] = _ao_anchor_block(coefficients[:, block], overlap)

    evaluator = MappedHamiltonianEvaluator(
        selected_orbitals,
        len(selected_indices),
        effective_pauli_threshold,
    )
    norm_before, terms_before = evaluator.evaluate(coefficients)
    current_norm = norm_before
    angle_grid = np.linspace(0.0, np.pi, angle_samples, endpoint=False)
    angle_step = np.pi / angle_samples

    for _ in range(max_sweeps):
        sweep_start_norm = current_norm
        for block in selected_blocks:
            if len(block) < 2:
                continue
            for left, right in combinations(range(len(block)), 2):
                block_coefficients = coefficients[:, block].copy()
                objective = PlaneRotationObjective(
                    evaluator,
                    coefficients,
                    block_coefficients,
                    block,
                    left,
                    right,
                )
                grid_norms = np.asarray(
                    [objective(float(angle)) for angle in angle_grid]
                )
                best_angle = float(angle_grid[int(np.argmin(grid_norms))])
                refined_angle, refined_norm = _golden_section_minimum(
                    objective,
                    best_angle - angle_step,
                    best_angle + angle_step,
                )
                if refined_norm < current_norm - improvement_tolerance:
                    coefficients[:, block] = block_coefficients @ _givens_rotation(
                        len(block), left, right, float(refined_angle % np.pi)
                    )
                    current_norm = refined_norm

        if current_norm >= sweep_start_norm - improvement_tolerance:
            break

    norm_after, terms_after = evaluator.evaluate(coefficients)
    return NaturalOrbitalCoordinateMinimizationResult(
        orbitals=_orbitals_with_coefficients(selected_orbitals, coefficients),
        selected_blocks=tuple(selected_blocks),
        coefficient_norm_before=norm_before,
        coefficient_norm_after=norm_after,
        effective_pauli_terms_before=terms_before,
        effective_pauli_terms_after=terms_after,
    )
