# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Lattice, observables and frozen-circuit catalogue shared by the sample's scripts.

This module imports only NumPy, so it works both in the pinned reference
environment (``requirements-reference.txt``) and with the preview ``qdk`` build.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Iterable, Sequence

import numpy as np


DIRECTORY = Path(__file__).resolve().parent
CIRCUITS = DIRECTORY / "fixtures" / "ising_circuits"
CASE_A = DIRECTORY / "fixtures" / "case_a_4x4"

SIZES = (4, 5, 6, 8, 10)
FIELDS = (0.5, 3.03)
J = 1.0

Term = tuple[str, list[int], float]


def bonds(size: int) -> list[tuple[int, int]]:
    """Nearest-neighbour bonds of the open size×size lattice, qubit q = y*size + x."""
    if size < 2:
        raise ValueError("The lattice needs at least 2×2 sites")
    horizontal = [(y * size + x, y * size + x + 1) for y in range(size) for x in range(size - 1)]
    vertical = [(y * size + x, (y + 1) * size + x) for y in range(size - 1) for x in range(size)]
    return horizontal + vertical


def magnetization_terms(size: int) -> list[Term]:
    """m_z = (1/n) Σ_q Z_q as (paulis, qubits, coefficient) terms."""
    n = size * size
    return [("Z", [q], 1 / n) for q in range(n)]


def correlation_terms(size: int) -> list[Term]:
    """C_ZZ = (1/|E|) Σ_<i,j> Z_i Z_j over nearest-neighbour bonds."""
    edges = bonds(size)
    return [("ZZ", [i, j], 1 / len(edges)) for i, j in edges]


def z_expectation(probabilities: np.ndarray, terms: Iterable[tuple[str, Sequence[int], complex]]) -> float:
    """Evaluate a Z-only Pauli sum on a little-endian (q0 least significant) distribution."""
    width = int(probabilities.size).bit_length() - 1
    if probabilities.ndim != 1 or probabilities.size != 1 << width:
        raise ValueError("Expected a one-dimensional distribution over 2**n basis states")
    index = np.arange(probabilities.size)
    total = 0.0
    for paulis, qubits, coefficient in terms:
        if set(paulis) != {"Z"} or len(paulis) != len(qubits):
            raise ValueError(f"Only Z products are supported here: {paulis!r}")
        if any(not 0 <= q < width for q in qubits):
            raise ValueError(f"Qubit outside the {width}-qubit register: {qubits!r}")
        if complex(coefficient).imag != 0:
            raise ValueError(f"Coefficients must be real: {coefficient!r}")
        parity = np.zeros(probabilities.size, dtype=np.int64)
        for q in qubits:
            parity ^= (index >> q) & 1
        total += complex(coefficient).real * float(np.sum(probabilities * (1 - 2 * parity)))
    return total


def field_label(field: float) -> str:
    return f"{field:g}"


def circuit_name(size: int, field: float) -> str:
    return f"ising_{size}x{size}_h{field_label(field)}.ll"


def circuit_path(size: int, field: float) -> Path:
    """The frozen measured QIR for (size, field); 4×4 at h=0.5 is the I1 Case A input."""
    if size not in SIZES or field not in FIELDS:
        raise ValueError(
            f"No frozen circuit for size={size}, h={field_label(field)}; "
            f"sizes {list(SIZES)}, fields {[field_label(f) for f in FIELDS]}"
        )
    if (size, field) == (4, 0.5):
        return CASE_A / "measured.ll"
    return CIRCUITS / circuit_name(size, field)


def cpu_references(directory: Path = CIRCUITS) -> dict[str, dict]:
    """The frozen 4×4 CPU references, keyed like ``"4x4_h3.03"``."""
    return json.loads((directory / "cpu_reference.json").read_text())


def cpu_reference(size: int, field: float, directory: Path = CIRCUITS) -> dict | None:
    return cpu_references(directory).get(f"{size}x{size}_h{field_label(field)}")


def frozen_cases(sizes: Sequence[int] = SIZES, fields: Sequence[float] = FIELDS) -> list[tuple[int, float]]:
    return [(size, field) for size in sizes for field in fields]
