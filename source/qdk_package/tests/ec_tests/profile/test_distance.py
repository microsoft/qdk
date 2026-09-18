"""Contracts for bounded distances and generic failure witnesses."""

from dataclasses import FrozenInstanceError
from itertools import product
import operator

import pytest

import qodec as qc

from qdk.ec import CodeProfile, Distance, FaultEvent, GadgetProfile, Pauli
from qdk.ec._analysis.distance_solvers import CustomBoundsSolver
from qdk.ec._analysis.odd_cycles import OddCycles
from qdk.ec._distance import (
    _copy_fault,
    _fault_product,
    _pauli_product,
    distance_result_of,
)


def result(lower: int | None, upper: int | None) -> Distance[str]:
    witness = (
        None
        if upper is None
        else Distance.Witness._create(("a",) * upper, product="".join)
    )
    return Distance._create(lower, upper, witness=witness)


def test_surface_and_opaque_construction() -> None:
    assert {name for name in dir(Distance) if not name.startswith("_")} == {
        "lower_bound",
        "upper_bound",
        "value",
        "is_exact",
        "witness",
        "witnesses",
        "Witness",
    }
    assert {name for name in dir(Distance.Witness) if not name.startswith("_")} == {
        "factors",
        "product",
    }
    with pytest.raises(TypeError, match="profile"):
        Distance()
    with pytest.raises(TypeError, match="distance results"):
        Distance.Witness()
    with pytest.raises(TypeError):
        bool(result(2, 2))
    with pytest.raises(TypeError):
        hash(result(2, 2))
    with pytest.raises(TypeError):
        iter(result(2, 2))
    with pytest.raises(TypeError):
        len(result(2, 2).witness)
    with pytest.raises(TypeError):
        iter(result(2, 2).witness)
    with pytest.raises((AttributeError, FrozenInstanceError, TypeError)):
        result(2, 2).lower_bound = 3


@pytest.mark.parametrize(
    "lower,upper,text",
    [(2, 2, "2"), (2, 4, "[2, 4]"), (3, None, "[3, \u221e]"), (None, None, "\u221e")],
)
def test_display_and_exact_value(lower, upper, text) -> None:
    distance = result(lower, upper)
    assert str(distance) == text
    assert f"{distance:>14}" == text.rjust(14)
    assert repr(distance).startswith(
        f"Distance(lower_bound={lower!r}, upper_bound={upper!r}, witness="
    )
    assert distance.is_exact == (lower == upper)
    if distance.is_exact:
        assert distance.value == lower
    else:
        with pytest.raises(ValueError, match="not exact"):
            _ = distance.value
    if upper is None:
        with pytest.raises(LookupError):
            _ = distance.witness
        assert list(distance.witnesses) == []
    with pytest.raises(ValueError):
        format(distance, "d")


@pytest.mark.parametrize(
    "operation",
    [operator.eq, operator.ne, operator.lt, operator.le, operator.gt, operator.ge],
)
@pytest.mark.parametrize("left_bounds", [(2, 2), (2, 4), (3, None), (None, None)])
@pytest.mark.parametrize("right_bounds", [(0, 0), (2, 2), (2, 4), (5, 5), (None, None)])
def test_comparisons_require_a_proven_answer(
    operation, left_bounds, right_bounds
) -> None:
    def possible_values(bounds):
        lower, upper = bounds
        if lower is None:
            return [float("inf")]
        return list(range(lower, (upper if upper is not None else 6) + 1)) + (
            [float("inf")] if upper is None else []
        )

    answers = {
        operation(left, right)
        for left, right in product(
            possible_values(left_bounds), possible_values(right_bounds)
        )
    }
    left = result(*left_bounds)
    right = result(*right_bounds)
    if len(answers) == 1:
        assert operation(left, right) == answers.pop()
    else:
        with pytest.raises(ValueError, match="unresolved"):
            operation(left, right)


def test_reflected_comparisons_and_unsupported_operands() -> None:
    distance = result(3, 5)
    assert distance > 2 and 2 < distance
    assert distance <= 5 and 5 >= distance
    assert distance != 2 and 2 != distance
    with pytest.raises(ValueError, match="unresolved"):
        _ = 4 < distance
    assert result(2, 2) == 2 == result(2, 2)
    assert result(None, None) > 10**1000
    assert Distance.__eq__(distance, "3") is NotImplemented
    assert Distance.__lt__(distance, 3.0) is NotImplemented
    assert distance != "3"
    with pytest.raises(TypeError):
        _ = distance < "3"


def test_generic_witness_equality_hash_and_snapshot() -> None:
    witness = Distance.Witness._create(("a", "b"), product="".join)
    same = Distance.Witness._create(("a", "b"), product="".join)
    combined = Distance.Witness._create(("ab",), product="".join)
    assert witness.product == combined.product == "ab"
    assert witness == same and hash(witness) == hash(same)
    assert witness != combined
    assert witness != object()
    assert str(witness) == "a; b"
    assert repr(witness) == "Distance.Witness(factors=('a', 'b'))"
    factors = ([1], [2])
    mutable = Distance.Witness._create(factors, product=lambda items: sum(items, []))
    factors[0].append(3)
    mutable.factors[0].append(4)
    mutable.product.append(5)
    assert mutable.factors == ([1], [2])
    assert mutable.product == [1, 2]


@pytest.mark.parametrize(
    "factors, expected", [((), "1"), (("a",), "a"), (("a", "b"), "a; b")]
)
def test_witness_text_lists_only_factors(
    factors: tuple[str, ...], expected: str
) -> None:
    witness = Distance.Witness._create(factors, product="".join)
    assert str(witness) == expected
    assert f"{witness}" == expected


def test_fault_witness_text_preserves_factor_grouping() -> None:
    quantum = FaultEvent.after(2, Pauli("X_0"))
    readout = FaultEvent.after(7, readout_flips=0)
    separate = Distance.Witness._create(
        (quantum, readout), product=_fault_product, copy=_copy_fault
    )
    combined = Distance.Witness._create(
        (quantum * readout,), product=_fault_product, copy=_copy_fault
    )

    assert separate.product == combined.product
    assert str(separate) == "X_0 after call 2; flip call 7 readout 0"
    assert str(combined) == "(X_0 after call 2; flip call 7 readout 0)"


def test_lazy_fresh_iterators_do_not_search_during_protocols() -> None:
    first = Distance.Witness._create(("a",), product="".join)
    calls = []

    def alternatives():
        calls.append("search")
        yield Distance.Witness._create(("b",), product="".join)
        raise RuntimeError("enumeration interrupted")

    distance = Distance._create(1, 1, witness=first, alternatives=alternatives)
    iterator = distance.witnesses
    assert distance == 1
    assert str(distance) == "1"
    assert f"{distance.witness}" == "a"
    assert "Distance.Witness" in repr(distance)
    assert distance.witness is first
    assert calls == []
    assert next(iterator) is first
    assert calls == []
    assert next(iterator).product == "b"
    assert calls == ["search"]
    with pytest.raises(RuntimeError, match="interrupted"):
        next(iterator)
    assert next(distance.witnesses) is first


def test_alternatives_use_original_columns_and_keep_fixed_bounds() -> None:
    problem = OddCycles([frozenset()] * 3, [frozenset({0})] * 3)
    factors = ["a", "b", "c"]
    distance = distance_result_of(
        problem, factors, solver="enumeration", exact=True, product="".join
    )
    factors.clear()
    assert [witness.product for witness in distance.witnesses] == ["a", "b", "c"]
    open_problem = OddCycles(
        [frozenset({0, 1}), frozenset({1, 2}), frozenset({0, 2})],
        [frozenset({0}), frozenset(), frozenset()],
    )
    bounded = distance_result_of(
        open_problem,
        ("a", "b", "c"),
        solver=CustomBoundsSolver(lambda *_: (2, 3, [0, 1, 2])),
        product="".join,
    )
    assert [witness.product for witness in bounded.witnesses] == ["abc"]
    assert (bounded.lower_bound, bounded.upper_bound) == (2, 3)


def test_fault_witness_keeps_readout_cancellation_and_phase() -> None:
    factors = (
        FaultEvent.after(3, Pauli("X"), readout_flips=0),
        FaultEvent.after(3, Pauli("Z"), readout_flips=[0, 1]),
    )
    witness = Distance.Witness._create(
        factors, product=_fault_product, copy=_copy_fault
    )
    assert witness.product == factors[0] * factors[1]
    assert witness.product._locations[3][1] == frozenset({1})
    assert witness.product._locations[3][0].phase == (Pauli("X") * Pauli("Z")).phase
    assert hash(witness) == hash(
        Distance.Witness._create(factors, product=_fault_product, copy=_copy_fault)
    )


def test_pauli_witness_preserves_phase_order_and_snapshot() -> None:
    factors = (Pauli("X"), Pauli("Z"))
    witness = Distance.Witness._create(factors, product=_pauli_product, copy=Pauli.copy)
    reversed_witness = Distance.Witness._create(
        tuple(reversed(factors)), product=_pauli_product, copy=Pauli.copy
    )
    expected = Pauli("X") * Pauli("Z")
    assert witness.product == expected
    assert witness.product.phase != reversed_witness.product.phase
    assert witness != reversed_witness
    factors[0].__imul__(Pauli("X"))
    witness.factors[0].__imul__(Pauli("X"))
    witness.product.__imul__(Pauli("Z"))
    assert witness.product == expected
    assert witness.factors == (Pauli("X"), Pauli("Z"))


def test_coset_alternatives_respect_the_requested_logical_parity() -> None:
    profile = CodeProfile(qc.Code("pair", [], ["X_0", "X_1"], ["Z_0", "Z_1"]))
    errors = [Pauli("X_0"), Pauli("X_1"), Pauli("Y_0"), Pauli("Y_1")]
    distance = profile.distance(errors=errors, coset_representative=Pauli("X_1"))
    assert distance == 1
    assert {witness.product for witness in distance.witnesses} == {
        Pauli("X_1"),
        Pauli("Y_1"),
    }


def test_gadget_alternatives_replay_without_simulation_during_iteration(
    monkeypatch,
) -> None:
    qubit = qc.instructions.BlockOperand("qubit")
    physical = qc.InstructionSet(
        "physical",
        blocks=[qc.instructions.Block("qubit", encodes=1)],
        instructions=[qc.Instruction("idle", inputs=[qubit], outputs=[qubit])],
    )
    circuit = qc.gadgets.Circuit(physical, "- idle: [0]", format="yaml")
    profile = GadgetProfile(circuit)
    faults = [FaultEvent.after(0, Pauli(basis)) for basis in ("X", "Y", "Z")]
    distance = profile.distance(faults=faults)
    expected = tuple(faults)
    faults.clear()
    circuit.source = "- idle: [1]"

    def unexpected_simulation(*args, **kwargs):
        raise AssertionError("witness iteration must not rerun simulation")

    with monkeypatch.context() as context:
        context.setattr("qdk.ec._profile.propagate_faults", unexpected_simulation)
        iterator = distance.witnesses
        assert next(iterator) == distance.witness
        witnesses = list(distance.witnesses)
        assert len(witnesses) == 3
        assert {witness.product for witness in witnesses} == set(expected)
        assert list(distance.witnesses) == witnesses
    effects = profile.effects_of([witness.product for witness in witnesses])
    assert all(
        any(error.weight for error in effect.output_error.values())
        for effect in effects
    )


@pytest.mark.parametrize(
    "lower,upper", [(-1, None), (3, 2), (None, 2), (True, None), (0.5, None)]
)
def test_invalid_bounds_are_rejected(lower, upper) -> None:
    with pytest.raises(ValueError):
        Distance._create(lower, upper)


def test_witness_choices_do_not_change_numeric_equality() -> None:
    first = Distance.Witness._create(("a",), product="".join)
    second = Distance.Witness._create(("b",), product="".join)
    assert first != second
    assert Distance._create(1, 1, witness=first) == Distance._create(
        1, 1, witness=second
    )
