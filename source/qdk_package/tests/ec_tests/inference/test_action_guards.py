"""Guarded action contracts through GadgetProfile's public surface."""

from __future__ import annotations

import json
from collections.abc import Sequence
from itertools import combinations_with_replacement, product

import pytest
import qodec as qc
from qodec.actions import Clifford, Condition, Observe, Pauli, Rotate, Stabilize
from qodec.gadgets import Circuit, Encoding
from qodec.instructions import Block, BlockOperand, Parameter

import qdk.ec as ec

Action = Clifford | Observe | Pauli | Rotate | Stabilize


def _action(kind: str, condition: Condition | None = None) -> Action:
    if kind == "Clifford":
        return Clifford({"X_0": "Z_0", "Z_0": "X_0"}, condition=condition)
    if kind == "Stabilize":
        return Stabilize(["Z_0"], condition=condition)
    if kind == "Pauli":
        return Pauli("X_0", condition=condition)
    if kind == "Rotate":
        return Rotate("Z_0", 0.25, condition=condition)
    raise ValueError(kind)


def _gadget(
    actions: Sequence[Action],
    *,
    arguments: dict[str, bool | int | str] | None = None,
    declared: Sequence[Action] | None = None,
) -> qc.Gadget:
    arguments = {} if arguments is None else arguments
    operand = BlockOperand("qubit")
    parameters = [Parameter(name, "bit") for name in arguments]
    implemented = qc.Instruction(
        "logical",
        inputs=[operand],
        outputs=[operand],
        action=list(actions if declared is None else declared),
        parameters=parameters,
    )
    physical = qc.InstructionSet(
        "physical",
        blocks=[Block("qubit", 1)],
        instructions=[
            qc.Instruction(
                "step",
                inputs=[operand],
                outputs=[operand],
                action=list(actions),
                parameters=parameters,
            )
        ],
    )
    encoding = Encoding(qc.Code("qubit", [], ["X_0"], ["Z_0"]), support=["0"])
    return qc.Gadget(
        implemented,
        Circuit(
            physical,
            json.dumps([{"step": {"operands": [0], "arguments": arguments}}]),
            format="yaml",
        ),
        inputs=[encoding],
        outputs=[encoding],
        readouts=[
            [f"circuit.readouts[{index}]"] for index in range(implemented.observe_count)
        ],
    )


@pytest.mark.parametrize("kind", ["Pauli"])
@pytest.mark.parametrize("invert", [False, True])
def test_constant_guards_control_realized_and_declared_actions(
    kind: str, invert: bool
) -> None:
    guarded = ec.GadgetProfile(_gadget([_action(kind, Condition([], invert=invert))]))
    expected = ec.GadgetProfile(_gadget([_action(kind)] if invert else []))
    assert guarded.action.is_equivalent_to(expected.action)
    assert guarded.objective is not None and expected.objective is not None
    assert guarded.objective.is_equivalent_to(expected.objective)
    assert guarded.action.why_not_equivalent_to(expected.action) == ""


@pytest.mark.parametrize("kind", ["Pauli"])
@pytest.mark.parametrize("enabled", [False, True, 0, 1])
@pytest.mark.parametrize("invert", [False, True])
def test_bound_bit_guards_control_gadget_and_bare_circuit_actions(
    kind: str, enabled: bool | int, invert: bool
) -> None:
    gadget = _gadget(
        [_action(kind, Condition(["enabled"], invert=invert))],
        arguments={"enabled": enabled},
    )
    expected = _gadget([_action(kind)] if bool(enabled) != invert else [])
    assert ec.GadgetProfile(gadget).action.is_equivalent_to(
        ec.GadgetProfile(expected).action
    )
    assert ec.GadgetProfile(gadget.circuit).action.is_equivalent_to(
        ec.GadgetProfile(expected.circuit).action
    )


@pytest.mark.parametrize("invert", [False, True])
@pytest.mark.parametrize(
    "values", [(False, False), (False, True), (True, False), (True, True)]
)
def test_guard_predicates_are_xored(values: tuple[bool, bool], invert: bool) -> None:
    gadget = _gadget(
        [_action("Pauli", Condition(["a", "b"], invert=invert))],
        arguments=dict(zip(("a", "b"), values)),
    )
    expected = _gadget([_action("Pauli")] if (values[0] ^ values[1]) != invert else [])
    assert ec.GadgetProfile(gadget).action.is_equivalent_to(
        ec.GadgetProfile(expected).action
    )


@pytest.mark.parametrize("kind", ["Pauli"])
@pytest.mark.parametrize("invert", [False, True])
def test_canceled_outcome_predicates_reduce_to_a_constant(
    kind: str, invert: bool
) -> None:
    prefix = [Observe(["Z_0"])]
    guarded = _gadget(
        [
            *prefix,
            _action(kind, Condition(["outcomes[0]", "outcomes[0]"], invert=invert)),
        ]
    )
    expected = _gadget([*prefix, *([_action(kind)] if invert else [])])
    assert ec.GadgetProfile(guarded).action.is_equivalent_to(
        ec.GadgetProfile(expected).action
    )


@pytest.mark.parametrize("kind", ["Clifford", "Stabilize"])
@pytest.mark.parametrize("member", ["action", "objective"])
def test_measurement_dependent_non_pauli_actions_fail_explicitly(
    kind: str, member: str
) -> None:
    profile = ec.GadgetProfile(
        _gadget([Observe(["Z_0"]), _action(kind, Condition(["outcomes[0]"]))])
    )
    with pytest.raises(NotImplementedError, match=f"conditional {kind}"):
        getattr(profile, member)


@pytest.mark.parametrize("invert", [False, True])
def test_local_measurement_guards_preserve_conditional_pauli_signs(
    invert: bool,
) -> None:
    measured = Observe(["Z_0"])
    guarded = ec.GadgetProfile(
        _gadget(
            [
                measured,
                Pauli("X_0", condition=Condition(["outcomes[0]"], invert=invert)),
            ]
        )
    )
    expected = ec.GadgetProfile(
        _gadget([measured, Stabilize(["Z_0"]), *([Pauli("X_0")] if invert else [])])
    )
    assert guarded.action.is_equivalent_to(expected.action)
    assert guarded.objective is not None and expected.objective is not None
    assert guarded.objective.is_equivalent_to(expected.objective)


def test_local_outcomes_do_not_reference_prior_calls_or_flags() -> None:
    operand = BlockOperand("qubit")
    physical = qc.InstructionSet(
        "physical",
        blocks=[Block("qubit", 1)],
        instructions=[
            qc.Instruction(
                "known_one",
                outputs=[operand],
                action=[Stabilize(["Z_0"]), Pauli("X_0"), Observe(["Z_0"])],
                flags=["flag"],
            ),
            qc.Instruction(
                "correct",
                inputs=[operand],
                outputs=[operand],
                action=[
                    Observe(["Z_0"]),
                    Pauli("X_0", condition=Condition(["outcomes[0]"])),
                ],
            ),
            qc.Instruction(
                "reset",
                inputs=[operand],
                outputs=[operand],
                action=[Observe(["Z_0"]), Stabilize(["Z_0"])],
            ),
        ],
    )
    actual = Circuit(
        physical, "- known_one: [0]\n- correct: [1]\n- correct: [2]", format="yaml"
    )
    expected = Circuit(
        physical, "- known_one: [0]\n- reset: [1]\n- reset: [2]", format="yaml"
    )
    assert ec.GadgetProfile(actual).action.is_equivalent_to(
        ec.GadgetProfile(expected).action
    )


@pytest.mark.parametrize("invert", [False, True])
def test_local_outcome_indexes_count_observations_not_hidden_resets(
    invert: bool,
) -> None:
    prefix = [Stabilize(["Z_0"]), Observe(["Z_0"]), Pauli("X_0"), Observe(["Z_0"])]
    actual = ec.GadgetProfile(
        _gadget(
            [*prefix, Pauli("X_0", condition=Condition(["outcomes[1]"], invert=invert))]
        )
    )
    expected = ec.GadgetProfile(_gadget([*prefix, *([] if invert else [Pauli("X_0")])]))
    assert actual.action.is_equivalent_to(expected.action)
    assert actual.objective is not None and expected.objective is not None
    assert actual.objective.is_equivalent_to(expected.objective)
    assert actual.checks == expected.checks
    assert actual.readouts == expected.readouts


def test_prior_circuit_readouts_do_not_make_future_local_outcomes_available() -> None:
    gadget = _gadget(
        [Pauli("X_0", condition=Condition(["outcomes[0]"])), Observe(["Z_0"])]
    )
    physical = gadget.circuit.instruction_set
    operand = BlockOperand("qubit")
    physical.instructions["probe"] = qc.Instruction(
        "probe", inputs=[operand], outputs=[operand], action=[Observe(["Z_0"])]
    )
    program = Circuit(physical, "- probe: [0]\n- step: [1]", format="yaml")
    with pytest.raises(ValueError, match="preceding instruction outcome"):
        _ = ec.GadgetProfile(program).action


@pytest.mark.parametrize("kind", ["Clifford", "Stabilize", "Rotate"])
@pytest.mark.parametrize("invert", [False, True])
@pytest.mark.parametrize("member", ["action", "objective"])
def test_non_pauli_conditions_are_rejected_even_when_constant(
    kind: str, invert: bool, member: str
) -> None:
    gadget = _gadget([_action(kind, Condition([], invert=invert))])
    with pytest.raises(NotImplementedError, match=f"conditional {kind}"):
        getattr(ec.GadgetProfile(gadget), member)
    with pytest.raises(NotImplementedError, match=f"conditional {kind}"):
        _ = ec.GadgetProfile(gadget.circuit).action


@pytest.mark.parametrize("kind", ["Clifford", "Stabilize", "Rotate"])
@pytest.mark.parametrize("enabled", [False, True, "enabled"])
def test_non_pauli_parameter_conditions_are_rejected(
    kind: str, enabled: bool | str
) -> None:
    gadget = _gadget(
        [_action(kind, Condition(["enabled"]))], arguments={"enabled": enabled}
    )
    for member in ("action", "objective"):
        with pytest.raises(NotImplementedError, match=f"conditional {kind}"):
            getattr(ec.GadgetProfile(gadget), member)


@pytest.mark.parametrize("predicate", ["outcomes[0]", "outcomes[2]"])
def test_guards_cannot_read_future_local_outcomes(predicate: str) -> None:
    profile = ec.GadgetProfile(
        _gadget([_action("Pauli", Condition([predicate])), Observe(["Z_0"])])
    )
    with pytest.raises(ValueError, match="preceding"):
        _ = profile.action


def test_unguarded_rotation_remains_unsupported() -> None:
    with pytest.raises(TypeError, match="Rotate"):
        _ = ec.GadgetProfile(_gadget([Rotate("Z_0", 0.25)])).action


@pytest.mark.parametrize("enabled", [False, True])
def test_guarded_actions_preserve_profile_checks_readouts_and_equivalence(
    enabled: bool,
) -> None:
    prefix = [Stabilize(["Z_0"])]
    measured = Observe(["Z_0"])
    actual = ec.GadgetProfile(
        _gadget([*prefix, _action("Pauli", Condition([], invert=enabled)), measured])
    )
    expected = ec.GadgetProfile(
        _gadget([*prefix, *([_action("Pauli")] if enabled else []), measured])
    )
    assert actual.checks == expected.checks
    assert actual.readouts == expected.readouts
    assert actual.is_equivalent_to(expected)
    assert actual.why_not_equivalent_to(expected) == ""


@pytest.mark.parametrize("kind", ["Clifford", "Stabilize"])
def test_measurement_dependent_bit_arguments_fail_explicitly(kind: str) -> None:
    gadget = _gadget(
        [_action(kind, Condition(["enabled"]))],
        arguments={"enabled": "circuit.readouts[0]"},
    )
    physical = gadget.circuit.instruction_set
    operand = BlockOperand("qubit")
    physical.instructions["probe"] = qc.Instruction(
        "probe", inputs=[operand], outputs=[operand], action=[Observe(["Z_0"])]
    )
    program = Circuit(
        physical,
        '- probe: [0]\n- step: [1, enabled: "circuit.readouts[0]"]',
        format="yaml",
    )
    with pytest.raises(NotImplementedError, match=f"conditional {kind}"):
        _ = ec.GadgetProfile(program).action


@pytest.mark.parametrize("enabled", [False, True])
def test_fault_propagation_engine_honors_the_same_guard(enabled: bool) -> None:
    from qdk.ec._analysis.propagation.interpreter import _FramePropagator, walk_program
    from qdk.ec._analysis.propagation.pauli import Pauli as FramePauli

    gadget = _gadget([_action("Pauli", Condition([], invert=enabled))])
    frames = _FramePropagator(1)
    frames.apply_pauli_to_shot(0, FramePauli("X_0"))
    walk_program(gadget.circuit, extra_engines=[frames])
    row = frames.measure(FramePauli("Z_0"))
    assert bool(frames.outcome_deltas[row, 0])


@pytest.mark.parametrize("matches", [False, True])
def test_audit_compares_effective_action_not_the_unguarded_operation(
    matches: bool,
) -> None:
    gadget = _gadget(
        [_action("Pauli", Condition([]))],
        declared=[] if matches else [_action("Pauli")],
    )
    logical = qc.InstructionSet(
        "logical", blocks=[Block("qubit", 1)], instructions=[gadget.implements]
    )
    codec = qc.Qodec(
        [
            qc.Layer(logical, codes={"qubit": gadget.inputs[0].code}, gadgets=[gadget]),
            qc.Layer(gadget.circuit.instruction_set),
        ]
    )
    report = ec.audit(codec)
    mismatches = report.by_rule().get("gadget/action-mismatch", ())
    assert bool(mismatches) is not matches


def _symbolic_profile(
    predicates: list[str], *, invert: bool = False
) -> ec.GadgetProfile:
    return ec.GadgetProfile(
        _gadget(
            [Pauli("X_0", condition=Condition(predicates, invert=invert))],
            arguments={name: name for name in predicates},
        )
    )


@pytest.mark.parametrize("member", ["action", "objective"])
def test_symbolic_parameter_if_and_unless_are_not_equivalent(member: str) -> None:
    odd = getattr(_symbolic_profile(["enabled"]), member)
    even = getattr(_symbolic_profile(["enabled"], invert=True), member)
    identity = getattr(ec.GadgetProfile(_gadget([])), member)
    assert odd.is_equivalent_to(getattr(_symbolic_profile(["enabled"]), member))
    assert not odd.is_equivalent_to(even)
    assert not even.is_equivalent_to(odd)
    assert odd.why_not_equivalent_to(even)
    assert not odd.is_equivalent_to(identity)
    assert "enabled" in str(odd)


def test_symbolic_parameter_names_and_xor_dependencies_are_preserved() -> None:
    a = _symbolic_profile(["a"]).action
    b = _symbolic_profile(["b"]).action
    xor = _symbolic_profile(["a", "b"]).action
    reversed_xor = _symbolic_profile(["b", "a"]).action
    assert not a.is_equivalent_to(b)
    assert not a.is_equivalent_to(xor)
    assert xor.is_equivalent_to(reversed_xor)
    assert _symbolic_profile(["a", "a"]).action.is_equivalent_to(
        ec.GadgetProfile(_gadget([])).action
    )


def test_repeated_symbolic_parameter_uses_share_the_same_bit() -> None:
    guarded = Pauli("X_0", condition=Condition(["enabled"]))
    profile = ec.GadgetProfile(
        _gadget([guarded, guarded], arguments={"enabled": "enabled"})
    )
    identity = ec.GadgetProfile(_gadget([]))
    assert profile.action.is_equivalent_to(identity.action)
    assert profile.objective is not None and identity.objective is not None
    assert profile.objective.is_equivalent_to(identity.objective)


def test_repeated_calls_share_the_same_symbolic_parameter() -> None:
    guarded = Pauli("X_0", condition=Condition(["enabled"]))
    gadget = _gadget(
        [guarded], arguments={"enabled": "enabled"}, declared=[guarded, guarded]
    )
    gadget.circuit.source = json.dumps(
        [{"step": {"operands": [0], "arguments": {"enabled": "enabled"}}}] * 2
    )
    profile = ec.GadgetProfile(gadget)
    assert profile.objective is not None
    assert profile.action.is_equivalent_to(profile.objective)
    assert profile.action.is_equivalent_to(ec.GadgetProfile(_gadget([])).action)


def test_parameter_bindings_align_declared_and_realized_symbolic_actions() -> None:
    gadget = _gadget(
        [Pauli("X_0", condition=Condition(["physical_bit"]))],
        arguments={"physical_bit": "switch"},
    )
    gadget.implements.parameters = [Parameter("enabled", "bit")]
    gadget.implements.action = [Pauli("X_0", condition=Condition(["enabled"]))]
    gadget.parameter_bindings = {"enabled": "circuit.source.switch"}
    profile = ec.GadgetProfile(gadget)
    assert profile.objective is not None
    assert profile.action.is_equivalent_to(profile.objective)


def test_parameter_binding_alias_does_not_shadow_another_parameter_name() -> None:
    gadget = _gadget(
        [Pauli("X_0", condition=Condition(["physical_bit"]))],
        arguments={"physical_bit": "b"},
    )
    gadget.implements.parameters = [Parameter("a", "bit"), Parameter("b", "bit")]
    gadget.implements.action = [Pauli("X_0", condition=Condition(["a"]))]
    gadget.parameter_bindings = {"a": "circuit.source.b", "b": "circuit.source.c"}
    profile = ec.GadgetProfile(gadget)
    assert profile.objective is not None
    assert profile.action.is_equivalent_to(profile.objective)


def test_parameter_dependence_is_retained_with_measurement_signs() -> None:
    measurement = Observe(["Z_0"])

    def profile(invert: bool) -> ec.GadgetProfile:
        return ec.GadgetProfile(
            _gadget(
                [
                    measurement,
                    Pauli(
                        "X_0",
                        condition=Condition(["enabled", "outcomes[0]"], invert=invert),
                    ),
                ],
                arguments={"enabled": "enabled"},
            )
        )

    odd, even = profile(False), profile(True)
    assert odd.objective is not None
    assert odd.action.is_equivalent_to(odd.objective)
    assert not odd.action.is_equivalent_to(even.action)


def test_profile_equivalence_preserves_symbolic_parameter_polarity() -> None:
    odd = _symbolic_profile(["enabled"])
    even = _symbolic_profile(["enabled"], invert=True)
    assert not odd.is_equivalent_to(even)
    assert odd.why_not_equivalent_to(even)


def test_observing_a_parameter_does_not_turn_it_into_an_anonymous_random_bit() -> None:
    def profile(invert: bool) -> ec.GadgetProfile:
        return ec.GadgetProfile(
            _gadget(
                [
                    Stabilize(["Z_0"]),
                    Pauli("X_0", condition=Condition(["enabled"], invert=invert)),
                    Observe(["Z_0"]),
                ],
                arguments={"enabled": "enabled"},
            )
        )

    odd, even = profile(False), profile(True)
    assert odd.objective is not None
    assert odd.action.is_equivalent_to(odd.objective)
    assert not odd.action.is_equivalent_to(even.action)


def test_symbolic_paulis_transport_through_unconditional_cliffords() -> None:
    hadamard = _action("Clifford")
    condition = Condition(["enabled"])
    before = ec.GadgetProfile(
        _gadget(
            [Pauli("X_0", condition=condition), hadamard],
            arguments={"enabled": "enabled"},
        )
    )
    after = ec.GadgetProfile(
        _gadget(
            [hadamard, Pauli("Z_0", condition=condition)],
            arguments={"enabled": "enabled"},
        )
    )
    assert before.is_equivalent_to(after)
    assert before.objective is not None and after.objective is not None
    assert before.objective.is_equivalent_to(after.objective)


@pytest.mark.parametrize("kind", ["Pauli", "Stabilize", "Observe"])
def test_channel_action_resolves_bound_pauli_expressions(kind: str) -> None:
    def action(operator: str) -> Action:
        if kind == "Pauli":
            return Pauli(operator)
        if kind == "Stabilize":
            return Stabilize([operator])
        return Observe([operator])

    gadget = _gadget([action("operator")], arguments={"operator": "-Z_0"})
    gadget.implements.parameters = [Parameter("operator", "pauli")]
    gadget.circuit.instruction_set.instructions["step"].parameters = [
        Parameter("operator", "pauli")
    ]
    expected = _gadget([action("-Z_0")])
    assert ec.GadgetProfile(gadget).action.is_equivalent_to(
        ec.GadgetProfile(expected).action
    )
    assert ec.GadgetProfile(gadget.circuit).action.is_equivalent_to(
        ec.GadgetProfile(expected.circuit).action
    )


@pytest.mark.parametrize("measured", [False, True])
def test_symbolic_comparison_agrees_with_every_concrete_bit_assignment(
    measured: bool,
) -> None:
    prefix = [Observe(["Z_0"])] if measured else []
    programs: list[list[Action]] = [
        [],
        [Pauli("X_0", condition=Condition(["a"]))],
        [Pauli("X_0", condition=Condition(["a"], invert=True))],
        [Pauli("X_0", condition=Condition(["b"]))],
        [Pauli("X_0", condition=Condition(["a", "b"]))],
        [
            Pauli("X_0", condition=Condition(["a"])),
            Pauli("X_0", condition=Condition(["b"])),
        ],
        [
            Pauli("X_0", condition=Condition(["a"])),
            Pauli("X_0", condition=Condition(["a"])),
        ],
        [Pauli("Y_0", condition=Condition(["a"]))],
    ]
    symbolic = [
        ec.GadgetProfile(
            _gadget([*prefix, *steps], arguments={"a": "a", "b": "b"})
        ).action
        for steps in programs
    ]
    concrete = [
        [
            ec.GadgetProfile(
                _gadget([*prefix, *steps], arguments={"a": a, "b": b})
            ).action
            for a, b in product((False, True), repeat=2)
        ]
        for steps in programs
    ]
    for first, second in combinations_with_replacement(range(len(programs)), 2):
        expected = all(
            left.is_equivalent_to(right)
            for left, right in zip(concrete[first], concrete[second])
        )
        assert symbolic[first].is_equivalent_to(symbolic[second]) == expected, (
            first,
            second,
        )
        assert (
            symbolic[first].why_not_equivalent_to(symbolic[second]) == ""
        ) == expected
