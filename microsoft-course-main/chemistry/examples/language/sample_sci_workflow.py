"""Sample sparse-CI finder workflow combining QDK/Chemistry primitives with PMC.

This script performs a complete SCF → CASCI → sparse-CI finder sequence for a
user provided geometry and reports the determinant subset that reproduces the
CASCI energy to within a specific accuracy (default 1 mHartree).
The CLI exposes knobs for the initial valence active space selection (number of electrons and orbitals),
active-space solver (including the MACIS ASCI solver), sparse-CI
tolerance, and maximum determinant budget so users can explore different
heuristics without editing the code.
"""

# --------------------------------------------------------------------------------------------
# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the MIT License. See LICENSE.txt in the project root for license information.
# --------------------------------------------------------------------------------------------

import argparse
import json
from collections.abc import Sequence
from pathlib import Path

from qdk_chemistry.algorithms import create
from qdk_chemistry.data import Structure
from qdk_chemistry.utils import Logger, compute_valence_space_parameters
from qdk_chemistry.utils.wavefunction import (
    calculate_sparse_wavefunction,
    get_active_determinants_info,
)

DEFAULT_ENERGY_TOLERANCE = 1.0e-3  # Hartree
DEFAULT_MAX_DETERMINANTS = 2000


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    """Parse CLI options in the same order as the workflow steps."""
    parser = argparse.ArgumentParser(description="End-to-end sparse-CI finder demo")
    parser.add_argument(
        "--xyz",
        type=Path,
        help="Path to an XYZ geometry file. Defaults to examples/data/water.structure.xyz.",
    )
    parser.add_argument(
        "--basis",
        default="cc-pvdz",
        help="Basis set applied to the SCF solver (default: cc-pvdz).",
    )
    parser.add_argument(
        "--charge", type=int, default=0, help="Total molecular charge (default: 0)."
    )
    parser.add_argument(
        "--spin",
        type=int,
        default=1,
        help="Spin multiplicity (2S+1). Default assumes a singlet (1).",
    )
    parser.add_argument(
        "--num-active-electrons",
        type=int,
        default=None,
        help="Override the heuristic valence electron count (optional).",
    )
    parser.add_argument(
        "--num-active-orbitals",
        type=int,
        default=None,
        help="Override the heuristic valence orbital count (optional).",
    )
    parser.add_argument(
        "--initial-active-space-solver",
        choices=["macis_asci", "macis_cas"],
        default="macis_cas",
        help="Initial CASCI solver (default: macis_cas).",
    )
    parser.add_argument(
        "--autocas",
        action="store_true",
        help="Run autoCAS active space refinement after the initial CASCI step.",
    )
    parser.add_argument(
        "--autocas-parameters",
        type=str,
        default=None,
        help=(
            "JSON object with autoCAS overrides (only used when --autocas is supplied). "
            "e.g., '{\"entropy_threshold\": 0.01}'"
        ),
    )
    parser.add_argument(
        "--energy-tolerance",
        type=float,
        default=DEFAULT_ENERGY_TOLERANCE,
        help="Target agreement between sparse-CI and CASCI energies in Hartree (default: 1e-3).",
    )
    parser.add_argument(
        "--max-determinants",
        type=int,
        default=DEFAULT_MAX_DETERMINANTS,
        help="Maximum number of determinants retained during sparse-CI ranking (default: 2000).",
    )
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> None:
    """Drive the simplified SCF → CASCI → sparse-CI workflow."""
    args = parse_args(argv)

    ########################################################################################
    # 1. Load the target structure (fallback to the water example bundled with these demos).
    ########################################################################################
    default_structure = (
        Path(__file__).resolve().parent.parent / "data" / "water.structure.xyz"
    )
    structure_path = args.xyz or default_structure
    if not structure_path.is_file():
        raise FileNotFoundError(f"XYZ file {structure_path} not found.")
    structure = Structure.from_xyz_file(structure_path)
    Logger.info(structure.get_summary())

    ########################################################################################
    # 2. Run the SCF stage to obtain the reference wavefunction.
    ########################################################################################
    scf_solver = create("scf_solver")
    e_scf, scf_wavefunction = scf_solver.run(
        structure, args.charge, args.spin, args.basis
    )
    Logger.info(f"SCF energy = {e_scf:.8f} Hartree")

    ########################################################################################
    # 3. Select the valence active space (heuristic or user overrides).
    ########################################################################################
    inferred_e, inferred_orb = compute_valence_space_parameters(
        scf_wavefunction, args.charge
    )
    electrons = (
        args.num_active_electrons
        if args.num_active_electrons is not None
        else inferred_e
    )
    orbitals = (
        args.num_active_orbitals
        if args.num_active_orbitals is not None
        else inferred_orb
    )

    selector = create("active_space_selector", "qdk_valence")
    settings = selector.settings()
    settings.set("num_active_electrons", electrons)
    settings.set("num_active_orbitals", orbitals)

    active_orbital_wavefunction = selector.run(scf_wavefunction)
    active_orbitals = active_orbital_wavefunction.get_orbitals()
    Logger.info(active_orbitals.get_summary())

    ########################################################################################
    # 4. Build the active-space Hamiltonian.
    ########################################################################################
    hamiltonian_constructor = create("hamiltonian_constructor")
    active_hamiltonian = hamiltonian_constructor.run(active_orbitals)
    Logger.info(active_hamiltonian.get_summary())

    ########################################################################################
    # 5. Run the initial CASCI calculation.
    ########################################################################################
    casci_calculator = create(
        "multi_configuration_calculator", args.initial_active_space_solver
    )
    casci_calculator.settings().set("calculate_one_rdm", True)
    casci_calculator.settings().set("calculate_two_rdm", True)
    # Use fixed core selection for ASCI when starting from HF, since
    # percentage-based selection doesn't work well with single-determinant start
    if args.initial_active_space_solver == "macis_asci":
        casci_calculator.settings().set("core_selection_strategy", "fixed")
    e_cas, wfn_cas = casci_calculator.run(
        active_hamiltonian, *active_orbital_wavefunction.get_active_num_electrons()
    )
    Logger.info(f"CASCI energy = {e_cas:.8f} Hartree")

    ########################################################################################
    # 6. Optional autoCAS refinement of active space size.
    ########################################################################################
    if args.autocas:
        autocas_selector = create("active_space_selector", "qdk_autocas")
        if args.autocas_parameters:
            overrides = json.loads(args.autocas_parameters)
            for key, value in overrides.items():
                autocas_selector.settings().set(key, value)
        refined_wfn = autocas_selector.run(wfn_cas)
        indices, _ = refined_wfn.get_orbitals().get_active_space_indices()
        Logger.info(f"autoCAS selected active space with indices: {indices}")
        if len(indices) == 0:
            Logger.warn(
                "autoCAS did not identify correlated orbitals; retaining the initial space."
            )
        else:
            refined_orbitals = refined_wfn.get_orbitals()
            active_hamiltonian = hamiltonian_constructor.run(refined_orbitals)
            e_cas, wfn_cas = casci_calculator.run(
                active_hamiltonian, *refined_wfn.get_active_num_electrons()
            )
            Logger.info(active_hamiltonian.get_summary())
            Logger.info(f"autoCAS energy = {e_cas:.8f} Hartree")

    ########################################################################################
    # 7. Perform sparse-CI screening.
    ########################################################################################
    sparse_ci_energy, sparse_ci_wavefunction = calculate_sparse_wavefunction(
        reference_wavefunction=wfn_cas,
        hamiltonian=active_hamiltonian,
        reference_energy=e_cas,
        energy_tolerance=args.energy_tolerance,
        max_determinants=args.max_determinants,
    )

    Logger.info(f"Sparse CI energy = {sparse_ci_energy:.8f} Hartree")
    Logger.info(get_active_determinants_info(sparse_ci_wavefunction))


if __name__ == "__main__":
    Logger.set_global_level("info")
    main()
