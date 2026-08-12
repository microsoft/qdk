"""Tests for stabilizer-code distance estimation."""
from __future__ import annotations
from typing import Iterable
import operator
from functools import reduce
import pytest
from qdk.ec._analysis.stabilizer_code import StabilizerCode
from ec_tests.testing import code_catalog as catalog
from qdk.ec._analysis.propagation.pauli import Pauli
from qdk.ec.distance import (
    MwpfSolverOptions,
    code_distance_bounds_of,
    code_distance_of,
)
from ec_tests.testing.optional import requires_mwpf

exhaustive_cases: list[tuple[str, StabilizerCode, int]] = [
    ("five_qubit", catalog.make_five_qubit_code(), 3),
    ("steane", catalog.make_steane_code(), 3),
    ("shor", catalog.make_shor_code(), 3),
    ("repetition_3", catalog.make_repetition_code(3), 1),
    ("repetition_9", catalog.make_repetition_code(9), 1),
    ("hamming_3", catalog.make_quantum_hamming_code(3), 3),
    ("hamming_4", catalog.make_quantum_hamming_code(4), 3),
    ("extended_hamming_4", catalog.make_quantum_extended_hamming_code(4), 4),
    ("422", catalog.make_422_code(), 2),
    ("iceberg_8", catalog.make_iceberg_code(8), 2),
    ("color_832", catalog.make_color_code_832(), 2),
    ("tesseract", catalog.make_tesseract_code(), 4),
    ("carbon", catalog.make_carbon_code(), 4),
]

mwpf_cases: list[tuple[str, StabilizerCode, int]] = exhaustive_cases + [
    ("golay", catalog.make_quantum_golay_code(), 7),
    ("surface_3", catalog.make_rotated_surface_code(x_distance=3, z_distance=3), 3),
    ("surface_5", catalog.make_rotated_surface_code(x_distance=5, z_distance=5), 5),
]


@pytest.mark.parametrize("name, code, expected", exhaustive_cases)
def test_exhaustive_code_distance_matches_known_value(
    name: str, code: StabilizerCode, expected: int
) -> None:
    distance, witness = code_distance_of(code)
    assert distance == expected, name
    assert code.is_non_trivial_logical_error(product_of(witness))
    assert len(witness) == expected


@requires_mwpf
@pytest.mark.parametrize("name, code, expected", mwpf_cases)
def test_mwpf_upper_bound_matches_known_distance(
    name: str, code: StabilizerCode, expected: int
) -> None:
    lower, upper, witness = code_distance_bounds_of(code, solver=MwpfSolverOptions())
    assert upper == expected, name
    assert lower <= upper
    assert code.is_non_trivial_logical_error(product_of(witness))


@requires_mwpf
@pytest.mark.parametrize("name, code, expected", exhaustive_cases)
def test_mwpf_agrees_with_exhaustive_oracle(
    name: str, code: StabilizerCode, expected: int
) -> None:
    exact, _ = code_distance_of(code)
    _, upper, _ = code_distance_bounds_of(code, solver=MwpfSolverOptions())
    assert upper == exact, name
    assert exact == expected, name


def test_per_basis_distance_for_css_code() -> None:
    code = catalog.make_steane_code()
    distance_x, error_x = code_distance_of(code, errors="X")
    distance_z, error_z = code_distance_of(code, errors="Z")
    assert distance_x == 3
    assert distance_z == 3
    assert code.is_non_trivial_logical_error(product_of(error_x))
    assert code.is_non_trivial_logical_error(product_of(error_z))


def test_distance_upper_bound_short_circuits_search() -> None:
    code = catalog.make_five_qubit_code()
    distance, witness = code_distance_of(code, distance_upper_bound=2)
    assert distance > 2
    assert witness == []

def product_of(paulis: Iterable[Pauli]) -> Pauli:
    return reduce(operator.mul, paulis, Pauli({}))

