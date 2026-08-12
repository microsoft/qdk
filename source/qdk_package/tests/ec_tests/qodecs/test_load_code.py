"""Tests for the algebraic profile view of qodec code artifacts."""

import pytest

from ec_tests.testing import code_catalog
from ec_tests.testing.qodecs import c4
from qdk.ec._analysis.propagation.pauli import Pauli
from qdk.ec._analysis.code_algebra import SubsystemCode


qodec = pytest.importorskip("qodec")


def test_sparse_pauli_parses_qodec_format() -> None:
    result = Pauli("X_0 Z_1 Y_2")
    assert result == Pauli({0: "X", 1: "Z", 2: "Y"})


def test_sparse_pauli_parses_single_qubit() -> None:
    result = Pauli("X_0")
    assert result == Pauli({0: "X"})


def test_load_c4_matches_iceberg() -> None:
    bundle = c4()
    loaded = SubsystemCode.from_qodec(bundle.codes["C4"])
    expected = code_catalog.make_422_code()

    assert loaded.logical_qubit_count == expected.logical_qubit_count
    assert loaded.length == expected.length
    assert set(loaded.support) == set(expected.support)
    _assert_same_stabilizer_group(loaded, expected)
    _assert_logicals_are_well_formed(loaded, expected)


def _assert_same_stabilizer_group(actual: SubsystemCode, expected: SubsystemCode) -> None:
    actual_group = actual.stabilizer
    expected_group = expected.stabilizer
    for generator in expected_group.generators:
        assert generator in actual_group, (
            f"expected stabilizer {generator} not in loaded code"
        )
    for generator in actual_group.generators:
        assert generator in expected_group, (
            f"loaded stabilizer {generator} not in expected code"
        )


def _assert_logicals_are_well_formed(actual: SubsystemCode, expected: SubsystemCode) -> None:
    """The loaded logical basis need not match the expected basis bit-for-bit
    (different valid bases describe the same code), but every loaded logical
    must commute with every expected stabilizer and act non-trivially as a
    logical operator on the expected code.
    """
    expected_stabilizers = expected.stabilizers
    for generator in actual.logical_basis:
        for stabilizer in expected_stabilizers:
            assert generator.commutes_with(stabilizer), (
                f"loaded logical {generator} does not commute with "
                f"expected stabilizer {stabilizer}"
            )
        assert expected.is_non_trivial_logical_error(generator), (
            f"loaded logical {generator} is trivial in the expected code"
        )
