"""Choose an active space for stretched N2."""

# --------------------------------------------------------------------------------------------
# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the MIT License. See LICENSE.txt in the project root for license information.
# --------------------------------------------------------------------------------------------

from dataclasses import dataclass
from typing import cast

from qdk_chemistry.algorithms import create
from qdk_chemistry.data import Structure, Wavefunction
from qdk_chemistry.data.symmetry import SymmetryLabel, axes
from qdk_chemistry.utils import Logger, compute_valence_space_parameters
from qdk_chemistry.utils.cubegen import generate_cubefiles_from_orbitals

Logger.set_global_level(Logger.LogLevel.off)


@dataclass
class ActiveSpaceResult:
    """Results needed by both the command-line example and visualization notebook."""

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
    inactive_indices: list[int]
    refined_indices: list[int]
    num_refined_electrons: int
    num_virtual_orbitals: int
    refined_energy: float
    refined_casci_wavefunction: Wavefunction
    num_refined_determinants: int


def run_active_space_workflow() -> ActiveSpaceResult:
    """Run the complete stretched-N2 active-space workflow."""
    ################################################################################
    # docs:xyz ../data/tutorial_stretched_n2.structure.xyz
    # start-cell-hartree-fock
    structure = Structure.from_xyz("""\
2
Stretched N2 molecule for the ground-state QPE tutorial
N    0.000000    0.000000    0.000000
N    0.000000    0.000000    1.850000
""")
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
    num_valence_alpha, num_valence_beta = valence_wavefunction.get_active_num_electrons()
    # end-cell-valence-space
    ################################################################################

    ################################################################################
    # start-cell-initial-casci
    hamiltonian_constructor = create("hamiltonian_constructor")
    casci_solver = create(
        "multi_configuration_calculator",
        "macis_cas",
        # autoCAS entropies require both one- and two-particle RDMs.
        calculate_one_rdm=True,
        calculate_two_rdm=True,
    )

    valence_hamiltonian = hamiltonian_constructor.run(valence_wavefunction.get_orbitals())
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
    natural_orbital_hamiltonian = hamiltonian_constructor.run(natural_orbital_wavefunction.get_orbitals())
    natural_orbital_energy, natural_orbital_casci_wavefunction = casci_solver.run(
        natural_orbital_hamiltonian,
        num_valence_alpha,
        num_valence_beta,
    )
    # Store ordinary Python floats rather than library scalar types so the
    # values can be printed and passed to the visualization notebook directly.
    orbital_entropies = [
        float(value) for value in natural_orbital_casci_wavefunction.get_single_orbital_entropies()
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
    inactive_indices = list(refined_orbitals.inactive_indices().indices(alpha_channel))
    num_refined_alpha, num_refined_beta = refined_wavefunction.get_active_num_electrons()
    num_refined_electrons = num_refined_alpha + num_refined_beta
    num_refined_orbitals = len(refined_indices)
    num_virtual_orbitals = (
        refined_orbitals.get_num_molecular_orbitals() - len(inactive_indices) - num_refined_orbitals
    )
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
        inactive_indices=inactive_indices,
        refined_indices=refined_indices,
        num_refined_electrons=num_refined_electrons,
        num_virtual_orbitals=num_virtual_orbitals,
        refined_energy=refined_energy,
        refined_casci_wavefunction=refined_casci_wavefunction,
        num_refined_determinants=num_refined_determinants,
    )


def print_active_space_results(result: ActiveSpaceResult) -> None:
    """Print the values students record in the tutorial lab notebook."""
    print(f"Hartree-Fock energy: {result.hartree_fock_energy:.12f} Hartree")
    print(
        f"Initial valence space: CAS({result.num_valence_electrons}e, "
        f"{result.num_valence_orbitals}o)"
    )
    print(f"Initial active orbital indices: {result.valence_indices}")
    print(f"Initial CASCI energy: {result.valence_energy:.12f} Hartree")
    print(f"Initial CASCI determinants: {result.num_valence_determinants}")
    print(f"Natural-orbital CASCI energy: {result.natural_orbital_energy:.12f} Hartree")
    orbital_transformation_energy_change = result.natural_orbital_energy - result.valence_energy
    print(
        "Energy change after the natural-orbital transformation: "
        f"{orbital_transformation_energy_change:.12e} Hartree"
    )
    print("Single-orbital entropies:")
    for orbital_index, entropy in zip(result.valence_indices, result.orbital_entropies, strict=True):
        selection_marker = "*" if orbital_index in result.refined_indices else " "
        print(f" {selection_marker} orbital {orbital_index}: {entropy:.9f}")
    print(f"Refined active space: CAS({result.num_refined_electrons}e, {len(result.refined_indices)}o)")
    print(f"Inactive orbital indices: {result.inactive_indices}")
    print(f"Active orbital indices: {result.refined_indices}")
    print(f"Virtual orbitals: {result.num_virtual_orbitals}")
    print(f"Final CASCI energy: {result.refined_energy:.12f} Hartree")
    print(f"Final CASCI determinants: {result.num_refined_determinants}")
    energy_increase = result.refined_energy - result.natural_orbital_energy
    print(f"Energy increase from reducing the active space: {energy_increase:.12f} Hartree")


def generate_active_orbital_cube_data(
    result: ActiveSpaceResult,
    grid_size: tuple[int, int, int] = (30, 30, 30),
    margin: float = 10.0,
) -> dict[str, dict]:
    """Generate widget data for all candidate orbitals and their selection evidence."""
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
        occupation = float(occupation_alpha[position]) + float(occupation_beta[position])
        cube_data[f"Orbital {orbital_index}"] = {
            "data": cube_file,
            "info": {
                "Occupation": f"{occupation:.3f}",
                "Entropy": f"{result.orbital_entropies[position]:.3f}",
                "Selected by autoCAS": "yes" if orbital_index in result.refined_indices else "no",
            },
        }
    return cube_data


def main() -> None:
    """Run and report the command-line version of the example."""
    # Earlier tutorial scripts execute one linear calculation. This chapter also
    # provides an interactive notebook, so functions keep both versions on the
    # same tested chemistry workflow instead of duplicating the calculation.
    result = run_active_space_workflow()
    print_active_space_results(result)


if __name__ == "__main__":
    main()