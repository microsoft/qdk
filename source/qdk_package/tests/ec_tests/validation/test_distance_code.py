"""Tests for stabilizer-code distance estimation."""

from __future__ import annotations
from typing import Iterable, Literal
from importlib import import_module
import operator
from functools import reduce
from unittest.mock import patch
import pytest
import qodec as qc
from qdk.ec import CodeProfile
from qdk.ec._analysis.stabilizer_code import StabilizerCode
from ec_tests.testing import code_catalog as catalog
from qdk.ec._analysis.propagation.pauli import Pauli
from qdk.ec._distance import (
    MwpfSolverOptions,
    code_distance_bounds_of,
    code_distance_of,
)
from ec_tests.testing.optional import requires_highs, requires_mwpf

enumeration_cases: list[tuple[str, StabilizerCode, int]] = [
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

mwpf_cases: list[tuple[str, StabilizerCode, int]] = enumeration_cases + [
    ("golay", catalog.make_quantum_golay_code(), 7),
    ("surface_3", catalog.make_rotated_surface_code(x_distance=3, z_distance=3), 3),
    ("surface_5", catalog.make_rotated_surface_code(x_distance=5, z_distance=5), 5),
]


@pytest.mark.parametrize("name, code, expected", enumeration_cases)
def test_enumeration_code_distance_matches_known_value(
    name: str, code: StabilizerCode, expected: int
) -> None:
    distance, witness = code_distance_of(code, solver="enumeration")
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
@pytest.mark.parametrize("name, code, expected", enumeration_cases)
def test_mwpf_agrees_with_enumeration_oracle(
    name: str, code: StabilizerCode, expected: int
) -> None:
    exact, _ = code_distance_of(code, solver="enumeration")
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
    with pytest.raises(RuntimeError, match="exact distance"):
        code_distance_of(code, distance_upper_bound=2)
    lower, upper, witness = code_distance_bounds_of(
        code, solver="enumeration", distance_upper_bound=2
    )
    assert lower == 3 and upper > lower and witness == []


@requires_highs
@pytest.mark.parametrize("method", ["distance", "distance_bounds"])
def test_code_profile_defaults_to_highs(method: str) -> None:
    profile = CodeProfile(
        qc.Code("rep3", ["Z_0 Z_1", "Z_1 Z_2"], ["X_0 X_1 X_2"], ["Z_0"])
    )
    with patch(
        "qdk.ec._analysis.distance_solvers.import_module", wraps=import_module
    ) as backend:
        distance = getattr(profile, method)(errors="X")
    assert distance == 3
    assert profile.is_non_trivial_logical_error(distance.witness.product)
    backend.assert_called_once_with("highspy")


@requires_highs
@pytest.mark.parametrize("solver", [None, "highs"])
@pytest.mark.parametrize("name, code, expected", enumeration_cases)
def test_highs_code_distance_matches_known_value(
    name: str, code: StabilizerCode, expected: int, solver: Literal["highs"] | None
) -> None:
    distance, witness = code_distance_of(code, solver=solver)
    lower, upper, bounded = code_distance_bounds_of(code, solver=solver)
    assert distance == lower == upper == expected, name
    assert len(witness) == len(bounded) == expected
    assert code.is_non_trivial_logical_error(product_of(witness))
    assert code.is_non_trivial_logical_error(product_of(bounded))


def test_distance_and_bounds_default_to_unit_cost_y_errors() -> None:
    code = CodeProfile(qc.Code("Y repetition", ["Y_0 Y_1"], ["Y_0"], ["X_0 X_1"]))
    for distance in (code.distance(), code.distance_bounds()):
        assert distance == 1
        assert len(distance.witness.factors) == 1
        assert distance.witness.product in (Pauli("Y_0"), Pauli("Y_1"))
        assert code.is_non_trivial_logical_error(distance.witness.product)
    distance, witness = code_distance_of(code)
    assert distance == len(witness) == 1
    lower, upper, witness = code_distance_bounds_of(code)
    assert lower == upper == len(witness) == 1
    assert code.distance(errors="XZ") == 2
    assert code.distance_bounds(errors="XZ") == 2
    assert code_distance_of(code, errors="XZ")[0] == 2
    assert code_distance_bounds_of(code, errors="XZ")[:2] == (2, 2)


def test_correlated_errors_remain_single_witness_factors() -> None:
    code = CodeProfile(
        qc.Code(
            "C4",
            ["X_0 X_1 X_2 X_3", "Z_0 Z_1 Z_2 Z_3"],
            ["X_0 X_1", "X_0 X_2"],
            ["Z_0 Z_2", "Z_0 Z_1"],
        )
    )
    errors = [Pauli("X_0 X_1")]
    for distance in (
        code.distance(errors=errors),
        code.distance_bounds(errors=errors),
    ):
        assert distance == 1
        assert distance.witness.factors == tuple(errors)
        assert distance.witness.product.weight == 2
    assert code_distance_of(code, errors=errors) == (1, errors)
    assert code_distance_bounds_of(code, errors=errors) == (1, 1, errors)


def product_of(paulis: Iterable[Pauli]) -> Pauli:
    return reduce(operator.mul, paulis, Pauli({}))
