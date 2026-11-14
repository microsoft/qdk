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

    assert profile.stabilizers == (SparsePauli("Z_0 Z_1"),)
    assert profile.x == (SparsePauli("X_0 X_1"),)
    assert profile.z == (SparsePauli("Z_0"),)
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


@pytest.mark.parametrize("name", ["stabilizers", "x", "z", "gauge_x", "gauge_z"])
def test_operator_collections_are_tuples_of_independent_copies(name: str) -> None:
    profile = ec.CodeProfile(qc.Code("gauge", ["Z_0 Z_1"], ["X_2"], ["Z_2"]))
    operators = getattr(profile, name)
    assert isinstance(operators, tuple)
    assert operators
    expected = tuple(pauli.copy() for pauli in operators)
    operators[0].__imul__(SparsePauli("X_9"))
    assert getattr(profile, name) == expected


def test_logical_axes_follow_qodec_order_and_can_have_any_physical_factors() -> None:
    profile = ec.CodeProfile(qc.Code("pair", [], ["Z_5", "Y_9"], ["X_5", "X_9"]))
    assert profile.x == (SparsePauli("Z_5"), SparsePauli("Y_9"))
    assert profile.z == (SparsePauli("X_5"), SparsePauli("X_9"))
    assert len(profile.x) == len(profile.z) == profile.logical_qubit_count == 2
    for index in range(profile.logical_qubit_count):
        assert profile.representative_of(SparsePauli({index: "X"})) == profile.x[index]
        assert profile.representative_of(SparsePauli({index: "Z"})) == profile.z[index]
    assert profile.gauge_x == profile.gauge_z == ()


def test_gauge_axes_preserve_pairs_and_commute_with_logical_operators() -> None:
    profile = ec.CodeProfile(
        qc.Code("gauge", ["Z_1 Z_3 Z_5 Z_7"], ["X_1 X_3"], ["Z_1"])
    )
    gauge_x, gauge_z = profile.gauge_x, profile.gauge_z
    assert len(gauge_x) == len(gauge_z) == 2
    for index, x in enumerate(gauge_x):
        for other_index, z in enumerate(gauge_z):
            assert x.commutes_with(z) is (index != other_index)
        assert all(x.commutes_with(other) for other in gauge_x)
    for z in gauge_z:
        assert all(z.commutes_with(other) for other in gauge_z)
    for gauge in gauge_x + gauge_z:
        assert set(gauge.support) <= profile.support
        assert all(
            gauge.commutes_with(operator)
            for operator in profile.stabilizers + profile.x + profile.z
        )


def test_profile_without_logical_qubits_exposes_empty_logical_axes() -> None:
    profile = ec.CodeProfile(qc.Code("gauge", ["Z_0 Z_1"], [], []))
    assert profile.x == profile.z == ()
    assert profile.logical_qubit_count == 0
    assert len(profile.gauge_x) == len(profile.gauge_z) == 1


def test_code_profile_preserves_physical_labels_and_generator_order() -> None:
    profile = ec.CodeProfile(
        qc.Code("sparse", ["Z_5 Z_9", "Z_1 Z_5"], ["X_1 X_5 X_9"], ["Z_1"])
    )
    assert profile.support == frozenset({1, 5, 9})
    assert profile.length == 3
    assert profile.logical_qubit_count == 1
    assert profile.stabilizers == (SparsePauli("Z_5 Z_9"), SparsePauli("Z_1 Z_5"))
    assert profile.syndrome_of(SparsePauli("X_9")) == frozenset({0})
    assert profile.syndrome_of(SparsePauli("X_1")) == frozenset({1})


@pytest.mark.parametrize(
    "error,syndrome,effect,is_logical",
    [
        ("I", frozenset(), "I", False),
        ("-I", frozenset(), "I", False),
        ("Z_0 Z_1", frozenset(), "I", False),
        ("X_0", frozenset({0}), "X", False),
        ("X_1", frozenset({0}), "I", False),
        ("Z_0", frozenset(), "Z", True),
        ("X_0 X_1", frozenset(), "X", True),
        ("-X_0 X_1", frozenset(), "X", True),
    ],
)
def test_logical_errors_require_zero_syndrome_and_a_nonidentity_effect(
    error: str, syndrome: frozenset[int], effect: str, is_logical: bool
) -> None:
    profile = ec.CodeProfile(repetition_code())
    pauli = SparsePauli(error)
    assert profile.syndrome_of(pauli) == syndrome
    assert profile.logical_effect_of(
        pauli, including_phase=False
    ) == SparsePauli(effect)
    assert profile.is_logical(pauli) is is_logical


@pytest.mark.parametrize("phase", ["", "-", "i", "-i"])
@pytest.mark.parametrize("logical", ["I", "X", "Y", "Z"])
def test_logical_representatives_round_trip_with_phase(
    phase: str, logical: str
) -> None:
    profile = ec.CodeProfile(repetition_code())
    pauli = SparsePauli(phase + logical)
    physical = profile.representative_of(pauli)
    assert profile.logical_effect_of(physical) == pauli
    assert profile.logical_effect_of(physical, including_phase=False) == abs(pauli)
    assert profile.logical_effect_of(physical * profile.stabilizers[0]) == pauli


def test_generated_stabilizer_signs_are_preserved() -> None:
    profile = ec.CodeProfile(qc.Code("bell", ["X_0 X_1", "Z_0 Z_1"], [], []))
    assert profile.logical_effect_of(SparsePauli("Y_0 Y_1")) == SparsePauli("-I")
    assert not profile.is_logical(SparsePauli("Y_0 Y_1"))


@pytest.mark.parametrize("error,effect", [("-X_0", "X"), ("-X_1", "I")])
def test_detectable_errors_require_explicit_phase_free_conversion(
    error: str, effect: str
) -> None:
    profile = ec.CodeProfile(repetition_code())
    with pytest.raises(ValueError, match="no logical action with a scalar phase"):
        profile.logical_effect_of(SparsePauli(error))
    assert profile.logical_effect_of(
        SparsePauli(error), including_phase=False
    ) == SparsePauli(effect)


@pytest.mark.parametrize(
    "error,effect,is_logical",
    [("Z_0", "I", False), ("X_0 X_1", "I", False), ("Z_0 X_2", "X", True)],
)
def test_gauge_components_have_no_unique_scalar_phase(
    error: str, effect: str, is_logical: bool
) -> None:
    profile = ec.CodeProfile(qc.Code("gauge", ["Z_0 Z_1"], ["X_2"], ["Z_2"]))
    pauli = SparsePauli(error)
    assert profile.is_logical(pauli) is is_logical
    with pytest.raises(ValueError, match="gauge component"):
        profile.logical_effect_of(pauli)
    assert profile.logical_effect_of(
        pauli, including_phase=False
    ) == SparsePauli(effect)


@pytest.mark.parametrize("method", ["syndrome_of", "logical_effect_of", "is_logical"])
def test_physical_error_queries_reject_out_of_support_labels(method: str) -> None:
    profile = ec.CodeProfile(repetition_code())
    with pytest.raises(ValueError, match="not supported"):
        getattr(profile, method)(SparsePauli("X_9"))
    with pytest.raises(TypeError, match="expected Pauli"):
        getattr(profile, method)("X_0")


def test_phase_free_errors_also_validate_physical_support() -> None:
    profile = ec.CodeProfile(repetition_code())
    with pytest.raises(ValueError, match="not supported"):
        profile.logical_effect_of(SparsePauli("X_9"), including_phase=False)


def test_representatives_use_logical_indexes_not_physical_labels() -> None:
    profile = ec.CodeProfile(qc.Code("sparse", ["Z_5 Z_9"], ["X_5 X_9"], ["Z_5"]))
    assert profile.representative_of(SparsePauli("X_0")) == SparsePauli("X_5 X_9")
    for index in (1, 5, 9):
        with pytest.raises(ValueError, match="no logical representative"):
            profile.representative_of(SparsePauli({index: "X"}))


@pytest.mark.parametrize("supported_by", [[0], [0, 0], [0, 1, 1], [0, 2]])
def test_encoding_requires_each_physical_label_exactly_once(
    supported_by: list[int],
) -> None:
    profile = ec.CodeProfile(repetition_code())
    with pytest.raises(ValueError, match="every physical support label once"):
        profile.encoding_clifford(supported_by=supported_by)


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
    assert profile.why_not_equivalent_to(swapped, strict_basis=False) == ""
    with pytest.raises(TypeError, match="unhashable"):
        hash(profile)


@pytest.mark.parametrize("including_signs", [False, True])
@pytest.mark.parametrize("strict_basis", [False, True])
def test_equivalence_and_explanation_use_the_same_sign_policy(
    including_signs: bool, strict_basis: bool
) -> None:
    left = ec.CodeProfile(qc.Code("left", ["X_0 X_1", "Z_0 Z_1"], [], []))
    right = ec.CodeProfile(qc.Code("right", ["X_0 X_1", "Y_0 Y_1"], [], []))
    options = dict(including_signs=including_signs, strict_basis=strict_basis)
    assert left.is_equivalent_to(right, **options) is (not including_signs)
    assert left.why_not_equivalent_to(right, **options) == (
        "Stabilizer groups differ." if including_signs else ""
    )
    assert not left.is_equivalent_to(right)
    assert left.why_not_equivalent_to(right) == "Stabilizer groups differ."


@pytest.mark.parametrize("including_signs", [False, True])
@pytest.mark.parametrize("strict_basis", [False, True])
def test_equivalence_compares_gauge_spaces(
    including_signs: bool, strict_basis: bool
) -> None:
    left = ec.CodeProfile(qc.Code("left", ["Z_0 Z_1"], ["X_2"], ["Z_2"]))
    right = ec.CodeProfile(qc.Code("right", ["Z_0 Z_1"], ["X_0 X_1 X_2"], ["Z_0"]))
    options = dict(including_signs=including_signs, strict_basis=strict_basis)
    assert not left.is_equivalent_to(right, **options)
    assert left.why_not_equivalent_to(right, **options) == "Gauge groups differ."


def test_reordered_stabilizers_are_equivalent_but_not_structurally_equal() -> None:
    left = ec.CodeProfile(
        qc.Code("left", ["Z_0 Z_1", "Z_1 Z_2"], ["X_0 X_1 X_2"], ["Z_0"])
    )
    right = ec.CodeProfile(
        qc.Code("right", ["Z_1 Z_2", "Z_0 Z_1"], ["X_0 X_1 X_2"], ["Z_0"])
    )
    assert left != right
    assert left.is_equivalent_to(right)
    assert left.why_not_equivalent_to(right) == ""


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
            errors="X", logical_observable=SparsePauli("Z"), solver="enumeration"
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
    assert profile.is_logical(distance.witness.product)
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
