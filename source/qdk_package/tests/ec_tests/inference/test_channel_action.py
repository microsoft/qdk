"""Tests for channel-action profiling."""

from __future__ import annotations

import pytest
import qodec as qc
from qodec.gadgets import Encoding

import qdk.ec as ec
from qdk.ec._analysis.channel_action import (
    ChannelAction,
    action_of,
    are_equivalent_mod_paulis,
    are_outcome_equivalent,
    declared_action_of,
    declared_program_of,
    gadget_action_mismatch,
    input_qubits_of,
    realized_action_of,
    realized_codes_of,
)
from qdk.ec._analysis.propagation.conditional import conditional_choi_state
from qdk.ec._analysis.propagation.interpreter import program_of
from qdk.ec._analysis.propagation.frames import FrameGroup, PauliFrame
from qdk.ec._analysis.propagation.pauli import Pauli
from qdk.ec._layout import ProgramLayout


def _action_of_gadget(gadget: qc.Gadget) -> ChannelAction:
    return action_of(program_of(gadget))


@pytest.mark.parametrize("decoded", [False, True])
def test_action_frames_use_simulator_outcomes(
    measure_zz_gadget: qc.Gadget,
    idle_gadget: qc.Gadget,
    prepare_xx_gadget: qc.Gadget,
    decoded: bool,
) -> None:
    for gadget in (measure_zz_gadget, idle_gadget, prepare_xx_gadget):
        program = program_of(gadget)
        if decoded:
            input_code, _ = realized_codes_of(gadget)
            input_qubits = sorted(input_code.support)
            projectors = tuple(input_code.stabilizers)
            action = realized_action_of(gadget)
        else:
            input_qubits = sorted(input_qubits_of(program))
            projectors = ()
            action = action_of(program)
        simulation = conditional_choi_state(
            program, input_qubits=input_qubits, codespace_projector=projectors
        ).simulation
        random_outcomes = simulation.random_outcome_indicator.support
        rows = list(simulation.outcome_matrix.rows)
        for random_bit, outcome in enumerate(random_outcomes):
            assert set(rows[outcome].support) == {random_bit}
            assert not simulation.outcome_shift[outcome]
        generators = (
            *action._stabilizers.generators,
            *action._observables.generators,
            *action._mapping.values(),
        )
        assert all(generator.frame <= set(random_outcomes) for generator in generators)


def test_input_qubits_of_idle_channel_is_nonempty(idle_gadget: qc.Gadget) -> None:
    program = program_of(idle_gadget)
    inputs = input_qubits_of(program)
    assert isinstance(inputs, frozenset)
    assert all(isinstance(qubit, int) for qubit in inputs)
    assert inputs <= frozenset(range(ProgramLayout.of(program).total_qubits))


def test_action_of_idle_channel_returns_channel_action(
    idle_gadget: qc.Gadget,
) -> None:
    action = _action_of_gadget(idle_gadget)
    assert isinstance(action, ChannelAction)
    assert isinstance(action._observables, FrameGroup)
    assert isinstance(action._stabilizers, FrameGroup)
    assert isinstance(action._mapping, dict)


def test_action_is_equivalent_to_itself(idle_gadget: qc.Gadget) -> None:
    action = _action_of_gadget(idle_gadget)
    assert action.is_equivalent_to(action)
    assert action.is_equivalent_to(action, modulo_paulis=True)
    assert are_equivalent_mod_paulis(action, action)
    assert are_outcome_equivalent(action, action)


@pytest.mark.parametrize(
    "expected_frames,actual_frames,negated_output,equivalent",
    [
        (((0,),), ((17,),), None, True),
        (((0,), (0,)), ((3,), (4,)), None, False),
        (((0,), (1,)), ((2,), (2, 7)), None, True),
        (((),), ((),), 0, False),
        (((0,),), ((3,),), 0, True),
        (((0,), (0,)), ((3,), (3,)), 1, False),
        (((0,), (1,), (0, 1)), ((3,), (4,), (3, 4)), None, True),
        (((0,), (1,), (0, 1)), ((3,), (4,), (5,)), None, False),
        (((0,), (1,), (0, 1)), ((3,), (4,), (3, 4)), 2, False),
        ((), (), None, True),
    ],
)
def test_action_comparison_and_explanation_agree_on_sign_relations(
    expected_frames: tuple[tuple[int, ...], ...],
    actual_frames: tuple[tuple[int, ...], ...],
    negated_output: int | None,
    equivalent: bool,
) -> None:
    expected = ChannelAction._create(
        FrameGroup([]),
        FrameGroup(
            PauliFrame(Pauli({index: "Z"}), frozenset(frame))
            for index, frame in enumerate(expected_frames)
        ),
        {},
    )
    actual = ChannelAction._create(
        FrameGroup([]),
        FrameGroup(
            PauliFrame(
                (
                    -Pauli({index: "Z"})
                    if index == negated_output
                    else Pauli({index: "Z"})
                ),
                frozenset(frame),
            )
            for index, frame in enumerate(actual_frames)
        ),
        {},
    )
    for left, right in ((expected, actual), (actual, expected)):
        assert left.is_equivalent_to(right) == equivalent
        assert are_outcome_equivalent(left, right) == equivalent
        assert (left.why_not_equivalent_to(right) == "") == equivalent
        assert left.is_equivalent_to(right, modulo_paulis=True)


def test_action_comparison_keeps_measurement_and_preparation_sign_correlations() -> (
    None
):
    expected = ChannelAction._create(
        FrameGroup([PauliFrame(Pauli("Z_0"), frozenset({2}))]),
        FrameGroup([PauliFrame(Pauli("Z_0"), frozenset({2}))]),
        {},
    )
    independent = ChannelAction._create(
        FrameGroup([PauliFrame(Pauli("Z_0"), frozenset({4}))]),
        FrameGroup([PauliFrame(Pauli("Z_0"), frozenset({6}))]),
        {},
    )
    assert expected.is_equivalent_to(independent, modulo_paulis=True)
    assert not expected.is_equivalent_to(independent)
    assert expected.why_not_equivalent_to(independent)


def test_action_comparison_retains_mapping_correction_convention() -> None:
    operator = Pauli("X_0")
    expected = ChannelAction._create(
        FrameGroup([]), FrameGroup([]), {operator: PauliFrame(operator)}
    )
    actual = ChannelAction._create(
        FrameGroup([]),
        FrameGroup([]),
        {operator: PauliFrame(-operator, frozenset({7}))},
    )
    assert expected.is_equivalent_to(actual)
    assert expected.why_not_equivalent_to(actual) == ""


def test_distinct_gadgets_are_not_equivalent(
    idle_gadget: qc.Gadget, measure_xx_gadget: qc.Gadget
) -> None:
    idle = _action_of_gadget(idle_gadget)
    measure = _action_of_gadget(measure_xx_gadget)
    assert not idle.is_equivalent_to(measure)
    assert not idle.is_equivalent_to(measure, modulo_paulis=True)
    assert not are_equivalent_mod_paulis(idle, measure)


def test_sign_flipped_action_is_mod_paulis_equivalent_but_not_outcome(
    idle_gadget: qc.Gadget,
) -> None:
    action = _action_of_gadget(idle_gadget)
    if not action._mapping:
        return
    flipped_mapping = {key: value * -1 for key, value in action._mapping.items()}
    flipped = ChannelAction._create(
        action._observables, action._stabilizers, flipped_mapping
    )
    assert are_equivalent_mod_paulis(action, flipped)
    assert flipped.is_equivalent_to(action, modulo_paulis=True)
    assert not are_outcome_equivalent(action, flipped)
    assert not flipped.is_equivalent_to(action)


def test_different_stabilizers_are_not_mod_paulis_equivalent(
    idle_gadget: qc.Gadget,
) -> None:
    action = _action_of_gadget(idle_gadget)
    extra = FrameGroup(
        list(action._stabilizers.generators) + [PauliFrame(Pauli({0: "Z"}))]
    )
    perturbed = ChannelAction._create(action._observables, extra, action._mapping)
    assert not are_equivalent_mod_paulis(action, perturbed)


def test_preparation_declared_stabilizers_are_deterministic(
    prepare_xx_gadget: qc.Gadget,
    prepare_zz_gadget: qc.Gadget,
) -> None:
    """A ``stabilize`` preparation must fix its stabilisers at a definite +1.

    Regression: the interpreter enacted ``stabilize P`` as a bare projective
    measurement, so an X-basis preparation (``P`` anticommutes with the |0>
    reset) left the prepared sign riding on the random projection outcome — a
    spurious frame on the *declared* action that made every prepare_x gadget mismatch
    its deterministic (reset + H) circuit. Z-basis preparations were
    unaffected because Z already stabilises |0>. Both must come out frame-free
    and audit-clean.
    """
    for gadget in (prepare_xx_gadget, prepare_zz_gadget):
        declared = declared_action_of(gadget)
        generators = declared._stabilizers.standardized().generators
        assert generators, "preparation fixes no stabilisers"
        assert all(not framed.frame for framed in generators), (
            "preparation left an outcome frame on its stabilisers; `stabilize` "
            "must deterministically prepare the +1 eigenstate"
        )
        assert gadget_action_mismatch(gadget) is None


def test_idle_declared_and_realized_actions_match_golden_values(
    idle_gadget: qc.Gadget,
) -> None:
    profile = ec.GadgetProfile(idle_gadget)

    assert str(profile.objective) == (
        "observables: FrameGroup(generators=())\n"
        "stabilizers: FrameGroup(generators=())\n"
        "mapping: {X: X^{0}, Z: Z, IX: IX^{2}, IZ: IZ}"
    )
    assert str(profile.action) == (
        "observables: FrameGroup(generators=())\n"
        "stabilizers: FrameGroup(generators=())\n"
        "mapping: {X: X^{4,6}, Z: Z, IX: IX^{2,6}, IZ: IZ}"
    )


def test_realized_action_is_invariant_under_equivalent_logical_representatives(
    idle_gadget: qc.Gadget,
) -> None:
    equivalent_code = qc.Code(
        "C4-alternate-basis",
        stabilizers=["X_0 X_1 X_2 X_3", "Z_0 Z_1 Z_2 Z_3"],
        x=["X_2 X_3", "X_1 X_3"],
        z=["Z_1 Z_3", "Z_2 Z_3"],
    )
    alternate = qc.Gadget(
        idle_gadget.implements,
        idle_gadget.circuit,
        inputs=[
            Encoding(equivalent_code, support=list(entry.support))
            for entry in idle_gadget.inputs
        ],
        outputs=[
            Encoding(equivalent_code, support=list(entry.support))
            for entry in idle_gadget.outputs
        ],
        checks=list(idle_gadget.checks),
        readouts=list(idle_gadget.readouts),
    )

    original = ec.GadgetProfile(idle_gadget)
    changed = ec.GadgetProfile(alternate)
    assert original.action.is_equivalent_to(changed.action)


def test_destructive_measurement_carries_no_logical_but_stays_distinguishable(
    measure_zz_gadget: qc.Gadget,
    measure_xx_gadget: qc.Gadget,
    prepare_zz_gadget: qc.Gadget,
) -> None:
    """Pins why ``_decode`` skips a logical with no image instead of raising.

    An empty mapping is the right answer for a destructive gadget, and the
    observables still separate it from the other basis and from a preparation.
    """
    measured = realized_action_of(measure_zz_gadget)

    assert not measured._mapping
    for other in (measure_xx_gadget, prepare_zz_gadget):
        assert not are_equivalent_mod_paulis(measured, realized_action_of(other))


def test_declared_program_binds_inputs_and_outputs_to_the_same_indices(
    idle_gadget: qc.Gadget,
) -> None:
    """Pins the reference side of the action check: both operand sets are 0..n-1."""
    (call,) = declared_program_of(idle_gadget).calls

    assert call.mnemonic == idle_gadget.implements.mnemonic
    assert call.operands == [0, 1]


def test_declared_circuit_preserves_parameter_names() -> None:
    operand = qc.instructions.BlockOperand("qubit")
    parameter = qc.instructions.Parameter("theta", "number")
    instruction = qc.Instruction(
        "rotate",
        inputs=[operand],
        outputs=[operand],
        parameters=[parameter],
        action=[qc.actions.Rotate("Z_0", "theta")],
    )
    instruction_set = qc.InstructionSet(
        "rotations",
        blocks=[qc.instructions.Block("qubit", encodes=1)],
        instructions=[instruction],
    )
    circuit = qc.gadgets.Circuit(
        instruction_set,
        "- rotate: {operands: [0], arguments: {theta: angle}}",
        format="yaml",
    )
    encoding = Encoding(
        qc.Code("qubit", stabilizers=[], x=["X_0"], z=["Z_0"]), support=["0"]
    )
    gadget = qc.Gadget(
        instruction,
        circuit,
        inputs=[encoding],
        outputs=[encoding],
        parameter_bindings={"theta": "angle"},
    )
    declared = declared_program_of(gadget)
    assert declared.instruction_set.instructions["rotate"].parameters == [parameter]
    assert declared.calls[0].arguments == {"theta": "theta"}


def test_action_explanation_identifies_missing_observable() -> None:
    expected = ChannelAction._create(
        FrameGroup([PauliFrame(Pauli({0: "X"}))]), FrameGroup([]), {}
    )
    actual = ChannelAction._create(
        FrameGroup([PauliFrame(Pauli({0: "Z"}))]), FrameGroup([]), {}
    )
    assert expected.why_not_equivalent_to(actual) == (
        "Expected measured logical observable X; absent from the circuit's group."
    )


def test_action_explanation_identifies_output_image() -> None:
    operator = Pauli({0: "X"})
    expected = ChannelAction._create(
        FrameGroup([]), FrameGroup([]), {operator: PauliFrame(operator)}
    )
    actual = ChannelAction._create(
        FrameGroup([]), FrameGroup([]), {operator: PauliFrame(Pauli({0: "Z"}))}
    )
    assert (
        expected.why_not_equivalent_to(actual)
        == "Logical X: expected X; circuit gives Z."
    )


def test_action_explanation_identifies_opposite_sign() -> None:
    expected = ChannelAction._create(
        FrameGroup([]), FrameGroup([PauliFrame(Pauli({0: "Z"}))]), {}
    )
    actual = ChannelAction._create(
        FrameGroup([]), FrameGroup([PauliFrame(-Pauli({0: "Z"}))]), {}
    )
    assert (
        expected.why_not_equivalent_to(actual)
        == "Opposite sign parity: output stabilizer Z."
    )


def test_action_explanation_identifies_variable_sign() -> None:
    expected = ChannelAction._create(
        FrameGroup([]), FrameGroup([PauliFrame(Pauli({0: "Z"}))]), {}
    )
    actual = ChannelAction._create(
        FrameGroup([]), FrameGroup([PauliFrame(Pauli({0: "Z"}), frozenset({0}))]), {}
    )
    assert expected.why_not_equivalent_to(actual) == (
        "Sign parity (output stabilizer Z): circuit varies with outcomes; expected is fixed."
    )
