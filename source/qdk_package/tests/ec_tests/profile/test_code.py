"""Code profiling accepts qodec's canonical code type."""

import qodec as qc
import pytest
from paulimer import SparsePauli

from ec_tests.testing.code_catalog import make_five_qubit_code
import qdk.ec as ec
from qdk.ec._analysis.code_algebra import (
    SubsystemCode,
    as_qodec_code,
    subsystem_code_of,
)
from qdk.ec._distance import code_distance_of


def repetition_code() -> qc.Code:
    return qc.Code(
        "repetition_2",
        stabilizers=["Z_0 Z_1"],
        x=["X_0 X_1"],
        z=["Z_0"],
    )


def test_syndrome_of_accepts_qodec_code() -> None:
    view = ec.CodeProfile(repetition_code())
    assert view.syndrome_of(SparsePauli({0: "X"})) == frozenset({0})


def test_code_profile_snapshots_operators() -> None:
    code = repetition_code()
    profile = ec.CodeProfile(code)
    code.stabilizers = ["X_0 X_1"]
    code.x = ["Z_0 Z_1"]
    code.z = ["X_0"]

    assert profile.stabilizers == [SparsePauli("Z_0 Z_1")]
    assert profile.logical_basis == [
        SparsePauli("X_0 X_1"),
        SparsePauli("Z_0"),
    ]
    assert profile.syndrome_of(SparsePauli("X_0")) == frozenset({0})


def test_code_profile_requires_a_qodec_code() -> None:
    with pytest.raises(TypeError, match="expected qodec.Code"):
        ec.CodeProfile([])

    with pytest.raises(TypeError, match="expected qodec.Code"):
        ec.CodeProfile(ec.CodeProfile(repetition_code()))

    assert not hasattr(ec.CodeProfile, "of")


def test_code_profile_does_not_expose_algebra_construction() -> None:
    profile = ec.CodeProfile(repetition_code())

    assert isinstance(profile._algebra, SubsystemCode)
    assert not isinstance(profile, SubsystemCode)
    for name in ("relocated", "_from_operators", "_initialize_operators"):
        assert not hasattr(profile, name)


@pytest.mark.parametrize(
    "name",
    [
        "stabilizer",
        "stabilizers",
        "anti_stabilizer",
        "anti_stabilizers",
        "gauge",
        "gauge_basis",
        "logical",
        "logical_basis",
        "support",
        "length",
        "logical_qubit_count",
    ],
)
def test_code_profile_properties_match_algebra(name: str) -> None:
    code = repetition_code()
    profile = ec.CodeProfile(code)
    algebra = subsystem_code_of(code)

    assert getattr(profile, name) == getattr(algebra, name)


@pytest.mark.parametrize(
    "name",
    [
        "syndrome_of",
        "logical_effect_of",
        "logical_action_of",
        "unsigned_logical_action_of",
        "is_trivial_error",
        "is_trivial_logical_error",
        "is_logical_error",
        "is_non_trivial_logical_error",
    ],
)
@pytest.mark.parametrize("error", ["I", "X_0", "Z_0", "X_0 X_1", "Z_0 Z_1"])
def test_code_profile_error_queries_match_algebra(name: str, error: str) -> None:
    code = repetition_code()
    profile = ec.CodeProfile(code)
    algebra = subsystem_code_of(code)
    pauli = SparsePauli(error)

    assert getattr(profile, name)(pauli) == getattr(algebra, name)(pauli)


def test_code_profile_representatives_and_encoding_match_algebra() -> None:
    code = repetition_code()
    profile = ec.CodeProfile(code)
    algebra = subsystem_code_of(code)

    for pauli in (SparsePauli("X"), SparsePauli("Y"), SparsePauli("Z")):
        assert profile.representative_of(pauli) == algebra.representative_of(pauli)
    assert profile.encoding_clifford() == algebra.encoding_clifford()
    assert profile.encoding_clifford(supported_by=[1, 0]) == algebra.encoding_clifford(
        supported_by=[1, 0]
    )


def test_code_profile_equality_and_equivalence_are_distinct() -> None:
    profile = ec.CodeProfile(repetition_code())
    clone = ec.CodeProfile(repetition_code())
    swapped = ec.CodeProfile(qc.Code("swapped", ["Z_0 Z_1"], ["Z_0"], ["X_0 X_1"]))

    assert profile == clone
    assert profile != swapped
    assert profile != profile._algebra
    assert profile.is_equivalent_to(clone, including_signs=True)
    assert not profile.is_equivalent_to(swapped)
    assert profile.is_equivalent_to(swapped, strict_basis=False)
    assert profile.why_not_equivalent_to(clone) == ""
    assert profile.why_not_equivalent_to(swapped) == "Logical bases differ."


def test_code_profile_distance_preserves_search_options() -> None:
    profile = ec.CodeProfile(repetition_code())

    distance = profile.distance(errors="X", solver="enumeration")
    assert distance == len(distance.witness.factors) == 2
    bounds = profile.distance_bounds(errors="X", solver="enumeration")
    assert bounds == len(bounds.witness.factors) == 2
    bounded_profile = ec.CodeProfile(
        as_qodec_code(make_five_qubit_code(), "five_qubit")
    )
    with pytest.raises(RuntimeError, match="exact distance"):
        bounded_profile.distance(upper_bound=2, solver="enumeration")
    bounds = bounded_profile.distance_bounds(upper_bound=2, solver="enumeration")
    assert bounds.lower_bound == 3 and bounds.upper_bound is None
    assert list(bounds.witnesses) == []
    assert (
        profile.distance(
            errors="X", coset_representative=SparsePauli("X"), solver="enumeration"
        )
        == 2
    )


def test_code_distance_of_accepts_qodec_code() -> None:
    distance, witness = code_distance_of(repetition_code(), errors="X")
    assert distance == 2
    assert len(witness) == 2


def test_distance_exposes_a_product_and_alternative_witnesses() -> None:
    profile = ec.CodeProfile(repetition_code())
    distance = profile.distance()

    assert isinstance(distance, ec.Distance)
    assert distance == 1
    assert distance.value == distance.lower_bound == distance.upper_bound == 1
    assert profile.is_non_trivial_logical_error(distance.witness.product)
    witnesses = list(distance.witnesses)
    assert witnesses[0] == distance.witness
    assert {witness.product for witness in witnesses} == {
        SparsePauli("Z_0"),
        SparsePauli("Z_1"),
    }
    assert list(distance.witnesses) == witnesses


def test_distance_represents_impossible_failure_without_a_sentinel() -> None:
    distance = ec.CodeProfile(repetition_code()).distance(errors=[])

    assert distance.is_exact
    assert distance.lower_bound is distance.upper_bound is distance.value is None
    assert distance > 1000
    assert str(distance) == "\u221e"
    assert list(distance.witnesses) == []
    with pytest.raises(LookupError):
        _ = distance.witness
