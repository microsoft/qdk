"""Choose and condition an active space for stretched N2.

The workflow progresses from Hartree--Fock orbitals through valence-space CASCI,
natural orbitals, entropy-based autoCAS refinement, and a deterministic choice
of coordinates inside occupation-degenerate natural-orbital subspaces. The last
step coordinate-minimizes the mapped Hamiltonian coefficient norm
``lambda = sum(abs(h_l))`` while preserving the selected orbital subspace and its
CASCI energy.
"""

# --------------------------------------------------------------------------------------------
# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the MIT License. See LICENSE.txt in the project root for license information.
# --------------------------------------------------------------------------------------------

from dataclasses import dataclass
from typing import TYPE_CHECKING, cast

from qdk_chemistry.algorithms import create
from qdk_chemistry.data import Orbitals, Structure, Wavefunction
from qdk_chemistry.data.symmetry import SymmetryLabel, axes
from qdk_chemistry.utils import Logger, compute_valence_space_parameters
from tutorial_orbital_coordinates import (
    NaturalOrbitalCoordinateMinimizationResult,
    coordinate_minimize_natural_orbital_coefficient_norm,
)

if TYPE_CHECKING:
    from matplotlib.figure import Figure


def create_stretched_n2_structure() -> Structure:
    """Create the molecular geometry shared by the tutorial workflow and notebook."""
    return Structure.from_xyz("""\
2
Stretched N2 molecule for the ground-state QPE tutorial
N    0.000000    0.000000    0.000000
N    0.000000    0.000000    1.850000
""")


@dataclass
class ActiveSpaceResult:
    """Results shared by later tutorial chapters and the visualization notebook.

    Attributes:
        structure: Molecular geometry used by every calculation.
        hartree_fock_energy: Restricted Hartree--Fock energy in Hartree.
        valence_wavefunction: Hartree--Fock wavefunction with initial valence
            active-space metadata attached.
        valence_indices: Spatial-orbital indices in the initial valence space.
        num_valence_electrons: Total active electrons in the valence space.
        num_valence_orbitals: Spatial orbitals in the valence space.
        valence_energy: Initial valence-space CASCI energy in Hartree.
        valence_casci_wavefunction: Correlated valence-space wavefunction whose
            reduced density matrices define the natural orbitals.
        num_valence_determinants: Determinants in the complete valence-space CI.
        natural_orbital_energy: CASCI energy after the natural-orbital rotation.
        natural_orbital_casci_wavefunction: Correlated wavefunction recomputed in
            the natural-orbital representation.
        orbital_entropies: One entropy per valence spatial orbital, ordered like
            ``valence_indices``.
        refined_wavefunction: autoCAS wavefunction carrying the selected orbital
            partition before coordinate minimization.
        refined_orbitals: Selected orbitals after coordinate minimization inside
            occupation-degenerate blocks.
        inactive_indices: Frozen doubly occupied spatial-orbital indices.
        refined_indices: Spatial-orbital indices retained as active by autoCAS.
        num_refined_electrons: Electrons in the selected active space.
        num_virtual_orbitals: Spatial orbitals held empty outside the active space.
        refined_energy: Selected-space CASCI energy in Hartree and the later
            algorithmic reference.
        refined_casci_wavefunction: Complete selected-space CASCI wavefunction in
            the coordinate-minimized orbital representation.
        num_refined_determinants: Determinants in the complete selected-space CI.
        natural_orbital_coordinate_minimization: Coordinate-minimization diagnostics.
    """

    structure: Structure
    hartree_fock_energy: float
    valence_wavefunction: Wavefunction
    valence_indices: list[int]
    num_valence_electrons: int
    num_valence_orbitals: int
    valence_energy: float
    valence_casci_wavefunction: Wavefunction
    num_valence_determinants: int
    natural_orbital_energy: float
    natural_orbital_casci_wavefunction: Wavefunction
    orbital_entropies: list[float]
    refined_wavefunction: Wavefunction
    refined_orbitals: Orbitals
    inactive_indices: list[int]
    refined_indices: list[int]
    num_refined_electrons: int
    num_virtual_orbitals: int
    refined_energy: float
    refined_casci_wavefunction: Wavefunction
    num_refined_determinants: int
    natural_orbital_coordinate_minimization: (
        "NaturalOrbitalCoordinateMinimizationResult"
    )


def run_active_space_workflow() -> ActiveSpaceResult:
    """Build the correlated molecular model used by later tutorial chapters.

    The workflow computes Hartree--Fock orbitals, forms a complete valence-space
    CASCI reference, diagonalizes its one-particle RDM to obtain natural
    orbitals, applies entropy-based autoCAS refinement, coordinate-minimizes the
    selected degenerate-orbital gauge, and solves the final selected-space CASCI
    problem.

    Returns:
        Energies, wavefunctions, orbital partitions, mapping diagnostics, and
        coordinate-minimized selected orbitals needed by mapping, state
        preparation, and visualization.
    """
    ################################################################################
    # docs:xyz ../data/tutorial_stretched_n2.structure.xyz
    # start-cell-hartree-fock
    structure = create_stretched_n2_structure()
    charge = 0
    spin_multiplicity = 1
    basis_set = "cc-pvdz"

    scf_solver = create("scf_solver", "qdk")
    hartree_fock_energy, hartree_fock_wavefunction = scf_solver.run(
        structure,
        charge=charge,
        spin_multiplicity=spin_multiplicity,
        basis_or_guess=basis_set,
    )
    # end-cell-hartree-fock
    ################################################################################

    ################################################################################
    # start-cell-valence-space
    num_valence_electrons, num_valence_orbitals = compute_valence_space_parameters(
        hartree_fock_wavefunction, charge
    )
    valence_selector = create(
        "active_space_selector",
        "qdk_valence",
        num_active_electrons=num_valence_electrons,
        num_active_orbitals=num_valence_orbitals,
    )
    valence_wavefunction = valence_selector.run(hartree_fock_wavefunction)

    # Restricted alpha and beta channels contain the same spatial-orbital indices,
    # so read one channel to count each spatial orbital once.
    alpha_channel = SymmetryLabel([axes.alpha()])
    valence_indices = list(
        valence_wavefunction.get_orbitals().active_indices().indices(alpha_channel)
    )
    num_valence_alpha, num_valence_beta = (
        valence_wavefunction.get_active_num_electrons()
    )
    # end-cell-valence-space
    ################################################################################

    ################################################################################
    # start-cell-initial-casci
    hamiltonian_constructor = create("hamiltonian_constructor")
    casci_solver = create(
        "multi_configuration_calculator",
        "macis_cas",
        # Tight convergence keeps the RDM-derived natural subspaces stable
        # across numerical backends before their orbital gauge is selected.
        ci_residual_tolerance=1e-10,
        # autoCAS entropies require both one- and two-particle RDMs.
        calculate_one_rdm=True,
        calculate_two_rdm=True,
    )

    valence_hamiltonian = hamiltonian_constructor.run(
        valence_wavefunction.get_orbitals()
    )
    valence_energy, valence_casci_wavefunction = casci_solver.run(
        valence_hamiltonian,
        num_valence_alpha,
        num_valence_beta,
    )
    num_valence_determinants = len(valence_casci_wavefunction.get_coefficients())
    # end-cell-initial-casci
    ################################################################################

    ################################################################################
    # start-cell-natural-orbitals
    # Rotate the valence orbitals using the CASCI one-particle RDM so each
    # natural orbital has a well-defined correlated occupation.
    natural_orbital_localizer = create("orbital_localizer", "qdk_natural_orbitals")
    natural_orbital_wavefunction = natural_orbital_localizer.run(
        valence_casci_wavefunction,
        valence_indices,
        valence_indices,
    )

    # Rebuild and solve in the rotated basis so the RDMs and orbital entropies
    # describe the same natural-orbital representation.
    natural_orbital_hamiltonian = hamiltonian_constructor.run(
        natural_orbital_wavefunction.get_orbitals()
    )
    natural_orbital_energy, natural_orbital_casci_wavefunction = casci_solver.run(
        natural_orbital_hamiltonian,
        num_valence_alpha,
        num_valence_beta,
    )
    # Store ordinary Python floats rather than library scalar types so the
    # values can be printed and passed to the visualization notebook directly.
    orbital_entropies = [
        float(value)
        for value in natural_orbital_casci_wavefunction.get_single_orbital_entropies()
    ]
    # end-cell-natural-orbitals
    ################################################################################

    ################################################################################
    # start-cell-refine
    # autoCAS uses the RDM-derived orbital entropies to retain the orbitals that
    # carry the strongest correlation in a smaller active space.
    autocas_selector = create("active_space_selector", "qdk_autocas_eos")
    refined_wavefunction = autocas_selector.run(natural_orbital_casci_wavefunction)
    refined_orbitals = refined_wavefunction.get_orbitals()

    # Summarize the inactive, selected active, and virtual spatial-orbital spaces.
    alpha_channel = SymmetryLabel([axes.alpha()])
    refined_indices = list(refined_orbitals.active_indices().indices(alpha_channel))
    if not refined_indices:
        raise RuntimeError(
            "autoCAS selected no active orbitals. Set refined_wavefunction to "
            "natural_orbital_wavefunction to retain the complete valence space, "
            "or adjust the autoCAS thresholds before continuing."
        )
    inactive_indices = list(refined_orbitals.inactive_indices().indices(alpha_channel))
    num_refined_alpha, num_refined_beta = (
        refined_wavefunction.get_active_num_electrons()
    )
    num_refined_electrons = num_refined_alpha + num_refined_beta
    num_refined_orbitals = len(refined_indices)
    num_virtual_orbitals = (
        refined_orbitals.get_num_molecular_orbitals()
        - len(inactive_indices)
        - num_refined_orbitals
    )

    # Natural occupations can be degenerate, leaving the corresponding orbital
    # vectors free to rotate. Choose that gauge only after autoCAS has selected
    # the final active subspace, because lambda belongs to its mapped Hamiltonian.
    coordinate_minimization = coordinate_minimize_natural_orbital_coefficient_norm(
        valence_casci_wavefunction,
        refined_orbitals,
        valence_indices,
    )
    refined_orbitals = coordinate_minimization.orbitals
    # end-cell-refine
    ################################################################################

    ################################################################################
    # start-cell-final-casci
    refined_hamiltonian = hamiltonian_constructor.run(refined_orbitals)
    refined_energy, refined_casci_wavefunction = casci_solver.run(
        refined_hamiltonian,
        num_refined_alpha,
        num_refined_beta,
    )
    num_refined_determinants = len(refined_casci_wavefunction.get_coefficients())
    # end-cell-final-casci
    ################################################################################

    return ActiveSpaceResult(
        structure=structure,
        hartree_fock_energy=hartree_fock_energy,
        valence_wavefunction=valence_wavefunction,
        valence_indices=valence_indices,
        num_valence_electrons=num_valence_electrons,
        num_valence_orbitals=num_valence_orbitals,
        valence_energy=valence_energy,
        valence_casci_wavefunction=valence_casci_wavefunction,
        num_valence_determinants=num_valence_determinants,
        natural_orbital_energy=natural_orbital_energy,
        natural_orbital_casci_wavefunction=natural_orbital_casci_wavefunction,
        orbital_entropies=orbital_entropies,
        refined_wavefunction=refined_wavefunction,
        refined_orbitals=refined_orbitals,
        inactive_indices=inactive_indices,
        refined_indices=refined_indices,
        num_refined_electrons=num_refined_electrons,
        num_virtual_orbitals=num_virtual_orbitals,
        refined_energy=refined_energy,
        refined_casci_wavefunction=refined_casci_wavefunction,
        num_refined_determinants=num_refined_determinants,
        natural_orbital_coordinate_minimization=coordinate_minimization,
    )


def print_active_space_results(result: ActiveSpaceResult) -> None:
    """Print active-space evidence for the cumulative lab notebook.

    The output distinguishes energy invariance under the natural-orbital
    rotation, entropy-based orbital selection, coordinate-minimization
    diagnostics, and the energy cost of reducing the active space.

    Args:
        result: Completed active-space workflow.
    """
    print(f"Hartree-Fock energy: {result.hartree_fock_energy:.12f} Hartree")
    print(
        f"Initial valence space: CAS({result.num_valence_electrons}e, "
        f"{result.num_valence_orbitals}o)"
    )
    print(f"Initial active orbital indices: {result.valence_indices}")
    print(f"Initial CASCI energy: {result.valence_energy:.12f} Hartree")
    print(f"Initial CASCI determinants: {result.num_valence_determinants}")
    print(f"Natural-orbital CASCI energy: {result.natural_orbital_energy:.12f} Hartree")
    orbital_transformation_energy_change = (
        result.natural_orbital_energy - result.valence_energy
    )
    print(
        "Energy change after the natural-orbital transformation: "
        f"{orbital_transformation_energy_change:.12e} Hartree"
    )
    print("Single-orbital entropies:")
    for orbital_index, entropy in zip(
        result.valence_indices, result.orbital_entropies, strict=True
    ):
        selection_marker = "*" if orbital_index in result.refined_indices else " "
        print(f" {selection_marker} orbital {orbital_index}: {entropy:.9f}")
    print(
        f"Refined active space: CAS({result.num_refined_electrons}e, {len(result.refined_indices)}o)"
    )
    print(f"Inactive orbital indices: {result.inactive_indices}")
    print(f"Active orbital indices: {result.refined_indices}")
    print(f"Virtual orbitals: {result.num_virtual_orbitals}")
    print(
        "Degenerate selected orbital blocks: "
        f"{result.natural_orbital_coordinate_minimization.selected_blocks}"
    )
    print(
        "Mapped coefficient norm before/after coordinate minimization: "
        f"{result.natural_orbital_coordinate_minimization.coefficient_norm_before:.12f} / "
        f"{result.natural_orbital_coordinate_minimization.coefficient_norm_after:.12f} Hartree"
    )
    print(
        "Effective Pauli terms before/after coordinate minimization: "
        f"{result.natural_orbital_coordinate_minimization.effective_pauli_terms_before} / "
        f"{result.natural_orbital_coordinate_minimization.effective_pauli_terms_after}"
    )
    print(f"Final CASCI energy: {result.refined_energy:.12f} Hartree")
    print(f"Final CASCI determinants: {result.num_refined_determinants}")
    energy_increase = result.refined_energy - result.natural_orbital_energy
    print(
        f"Energy increase from reducing the active space: {energy_increase:.12f} Hartree"
    )


def plot_orbital_entropy_selection(result: ActiveSpaceResult) -> "Figure":
    """Plot entropy-ranked candidate orbitals and the autoCAS selection cut.

    The companion Jupyter notebook uses this helper to display and regenerate
    the entropy profile after students modify the shared workflow.

    Args:
        result: Active-space workflow result containing candidate orbital
            indices, single-orbital entropies, and selected orbital indices.

    Returns:
        A Matplotlib figure with orbitals sorted by decreasing entropy.

    Raises:
        ValueError: If the entropy data do not match the candidate orbitals or
            the selected orbitals do not form one entropy-ranked prefix.

    """
    import matplotlib.pyplot as plt

    if len(result.valence_indices) != len(result.orbital_entropies):
        raise ValueError(
            "Expected one single-orbital entropy for each candidate orbital."
        )

    ranked_orbitals = sorted(
        zip(result.valence_indices, result.orbital_entropies, strict=True),
        key=lambda item: item[1],
        reverse=True,
    )
    selected_indices = set(result.refined_indices)
    selected_flags = [
        orbital_index in selected_indices for orbital_index, _ in ranked_orbitals
    ]
    cut_position = len(selected_indices)
    expected_flags = [True] * cut_position + [False] * (
        len(ranked_orbitals) - cut_position
    )
    if selected_flags != expected_flags:
        raise ValueError(
            "The selected orbitals do not form a contiguous prefix when sorted "
            "by decreasing entropy."
        )

    ranks = list(range(len(ranked_orbitals)))
    orbital_indices = [orbital_index for orbital_index, _ in ranked_orbitals]
    entropies = [entropy for _, entropy in ranked_orbitals]
    selected_ranks = [
        rank for rank, selected in zip(ranks, selected_flags, strict=True) if selected
    ]
    excluded_ranks = [
        rank
        for rank, selected in zip(ranks, selected_flags, strict=True)
        if not selected
    ]

    figure, axis = plt.subplots(figsize=(7.2, 4.2), layout="constrained")
    axis.plot(ranks, entropies, color="#455A64", linewidth=1.5, zorder=1)
    if selected_ranks:
        axis.scatter(
            selected_ranks,
            [entropies[rank] for rank in selected_ranks],
            color="#00796B",
            marker="o",
            s=55,
            label="Selected by autoCAS",
            zorder=2,
        )
    if excluded_ranks:
        axis.scatter(
            excluded_ranks,
            [entropies[rank] for rank in excluded_ranks],
            color="#7B1FA2",
            marker="s",
            s=55,
            label="Excluded",
            zorder=2,
        )
    if 0 < cut_position < len(ranked_orbitals):
        axis.axvline(
            cut_position - 0.5,
            color="#5C6BC0",
            linestyle="--",
            linewidth=1.5,
            label="autoCAS cut",
        )

    axis.set_xticks(ranks, [str(index) for index in orbital_indices])
    axis.set_xlabel("Natural-orbital index (sorted by decreasing entropy)")
    axis.set_ylabel("Single-orbital entropy")
    axis.set_ylim(bottom=0.0)
    axis.spines[["top", "right"]].set_visible(False)
    axis.legend(frameon=False)
    return figure


def generate_basis_function_cube_data(
    structure: Structure,
    basis_name: str,
    grid_size: tuple[int, int, int] = (30, 30, 30),
    margin: float = 3.0,
) -> dict[str, dict]:
    """Generate notebook-viewer data for individual basis functions.

    The companion Jupyter notebook uses this helper to compare the localized
    basis functions with the natural molecular orbitals built from them. An
    identity AO coefficient matrix lets the existing orbital cube generator
    evaluate one basis function per column.

    Args:
        structure: Molecular geometry on which the basis functions are centered.
        basis_name: Name of the atomic-orbital basis set to evaluate.
        grid_size: Cube-grid points along each Cartesian direction. Tests use a
            smaller grid to reduce runtime; the default supports smooth viewing.
        margin: Additional cube-grid extent around the molecule in Bohr.

    Returns:
        A mapping from basis-function labels to cube-file text and display
        metadata for the molecular viewer.

    """
    import numpy as np
    from pyscf import gto
    from qdk_chemistry.plugins.pyscf.conversion import (
        pyscf_mol_to_qdk_basis,
        structure_to_pyscf_atom_labels,
    )
    from qdk_chemistry.utils.cubegen import generate_cubefiles_from_orbitals

    atoms, _, _ = structure_to_pyscf_atom_labels(structure)
    pyscf_molecule = gto.Mole(
        atom=atoms,
        basis=basis_name,
        charge=0,
        spin=0,
        unit="Bohr",
    )
    pyscf_molecule.build()
    basis_set = pyscf_mol_to_qdk_basis(pyscf_molecule, structure, basis_name)
    num_basis_functions = pyscf_molecule.nao_nr()
    basis_function_labels = [
        " ".join(label.split()[1:]) for label in pyscf_molecule.ao_labels()
    ]
    if len(basis_function_labels) != num_basis_functions:
        raise ValueError("Expected one atom-centered label for each basis function.")
    basis_functions = Orbitals(
        np.eye(num_basis_functions),
        None,
        None,
        basis_set,
    )
    raw_cube_data = cast(
        dict[str, str],
        generate_cubefiles_from_orbitals(
            orbitals=basis_functions,
            grid_size=grid_size,
            margin=margin,
            label_maker=lambda index: (
                f"Basis function {index}: {basis_function_labels[index]}"
            ),
        ),
    )
    cube_data = {}
    for index, (label, cube_file) in enumerate(raw_cube_data.items()):
        center, function_type = basis_function_labels[index].split(maxsplit=1)
        cube_data[label] = {
            "data": cube_file,
            "info": {
                "Representation": "Basis function",
                "Function index": str(index),
                "Center": center,
                "Function type": function_type,
            },
        }
    return cube_data


def generate_active_orbital_cube_data(
    result: ActiveSpaceResult,
    grid_size: tuple[int, int, int] = (30, 30, 30),
    margin: float = 10.0,
) -> dict[str, dict]:
    """Generate molecular-viewer data for every candidate natural orbital.

    Args:
        result: Completed workflow containing valence natural orbitals,
            occupations, entropies, and selected indices.
        grid_size: Cube-grid points along each Cartesian direction. Tests use a
            smaller grid to reduce runtime; the default supports smooth viewing.
        margin: Additional cube-grid extent around the molecule in Bohr.

    Returns:
        A mapping from display labels to cube-file text and an ``info`` mapping
        containing total spatial-orbital occupation, entropy, and autoCAS
        selection status. Total occupation is alpha plus beta occupation.
    """
    from qdk_chemistry.utils.cubegen import generate_cubefiles_from_orbitals

    wavefunction = result.natural_orbital_casci_wavefunction
    orbitals = wavefunction.get_orbitals()
    occupation_alpha, occupation_beta = wavefunction.get_active_orbital_occupations()

    # Occupation arrays use active-space positions, while cube-file labels use
    # the original molecular-orbital indices; this dictionary connects them.
    active_position = {
        orbital_index: position
        for position, orbital_index in enumerate(result.valence_indices)
    }
    raw_cube_data = cast(
        dict[str, str],
        generate_cubefiles_from_orbitals(
            orbitals=orbitals,
            grid_size=grid_size,
            margin=margin,
            indices=result.valence_indices,
        ),
    )

    cube_data = {}
    for raw_label, cube_file in raw_cube_data.items():
        # Cube labels number orbitals from one, while QDK/Chemistry indices start
        # from zero, so convert before looking up occupations and entropies.
        orbital_index = int(raw_label.split("_")[1]) - 1
        position = active_position[orbital_index]
        # Add the alpha and beta occupations to report the total occupation of
        # each spatial orbital in the viewer.
        occupation = float(occupation_alpha[position]) + float(
            occupation_beta[position]
        )
        cube_data[f"Orbital {orbital_index}"] = {
            "data": cube_file,
            "info": {
                "Occupation": f"{occupation:.3f}",
                "Entropy": f"{result.orbital_entropies[position]:.3f}",
                "Selected by autoCAS": "yes"
                if orbital_index in result.refined_indices
                else "no",
            },
        }
    return cube_data


def main() -> None:
    """Run the command-line workflow and print its lab-notebook evidence."""
    # Change ``off`` to ``info`` to see detailed QDK/Chemistry calculation logs.
    Logger.set_global_level(Logger.LogLevel.off)
    # Earlier tutorial scripts execute one linear calculation. This chapter also
    # provides an interactive notebook, so functions keep both versions on the
    # same tested chemistry workflow instead of duplicating the calculation.
    result = run_active_space_workflow()
    print_active_space_results(result)


if __name__ == "__main__":
    main()
