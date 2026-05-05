"""Utility functions for orbital visualization."""

# --------------------------------------------------------------------------------------------
# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the MIT License. See LICENSE.txt in the project root for license information.
# --------------------------------------------------------------------------------------------

from __future__ import annotations

from qdk_chemistry.data import Orbitals
from qdk_chemistry.utils.cubegen import generate_cubefiles_from_orbitals


def orbital_label(idx: int, n_occupied: int) -> str:
    """Return a human-readable label (e.g., HOMO-1, LUMO+2) for a molecular orbital index.

    Args:
        idx: Zero-based molecular orbital index.
        n_occupied: Number of occupied orbitals (HOMO index is ``n_occupied - 1``).

    Returns:
        Label string such as "HOMO", "HOMO-2", "LUMO", or "LUMO+1".

    """
    offset = idx - (n_occupied - 1)
    if offset == 0:
        return "HOMO"
    elif offset < 0:
        return f"HOMO{offset}"
    elif offset == 1:
        return "LUMO"
    else:
        return f"LUMO+{offset - 1}"


def generate_cube_data_with_info(
    orbitals: Orbitals,
    n_occupied: int,
    indices: list[int] | None = None,
    grid_size: tuple[int, int, int] = (40, 40, 40),
    margin: float = 10.0,
) -> dict[str, dict]:
    """Generate cube data with info overlays for the MoleculeViewer widget.

    Each entry in the returned dictionary is keyed by a human-readable orbital label
    (e.g., "HOMO-1") and contains ``"data"`` (cube file string) and ``"info"``
    (metadata dict with energy and occupation).

    Args:
        orbitals: Orbitals object containing MO coefficients and energies.
        n_occupied: Number of occupied orbitals.
        indices: Orbital indices to include. If None, all orbitals are included.
        grid_size: Grid dimensions for cube file generation.
        margin: Margin in Bohr around the molecule for the cube grid.

    Returns:
        Dictionary suitable for passing as ``cube_data`` to ``MoleculeViewer``.

    """
    if orbitals.is_unrestricted():
        raise ValueError(
            "generate_cube_data_with_info only supports restricted orbitals. "
            "Unrestricted orbitals have separate alpha/beta channels that require different handling."
        )

    energies = orbitals.get_energies_alpha()

    cube_data_raw = generate_cubefiles_from_orbitals(
        orbitals=orbitals,
        grid_size=grid_size,
        margin=margin,
        indices=indices,
    )

    cube_data_with_info: dict[str, dict] = {}
    for raw_label, cube_str in cube_data_raw.items():
        idx = int(raw_label.split("_")[1]) - 1
        cube_data_with_info[orbital_label(idx, n_occupied)] = {
            "data": cube_str,
            "info": {
                "Energy (Ha)": f"{energies[idx]:.4f}",
                "Occupation": "occupied" if idx < n_occupied else "virtual",
            },
        }

    return cube_data_with_info
