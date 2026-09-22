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
)
from qdk.ec._analysis.propagation.interpreter import program_of, propagate_faults
from qdk.ec._analysis.propagation.isa_actions import remap_pauli
from qdk.ec._analysis.propagation.frames import FrameGroup, PauliFrame
from qdk.ec._analysis.propagation.pauli import Pauli
from qdk.ec._build import _physical_isa
from qdk.ec._layout import ProgramLayout


def _action_of_gadget(gadget: qc.Gadget) -> ChannelAction:
    return action_of(program_of(gadget))


def test_remap_pauli_preserves_signed_observable() -> None:
    from qdk.ec._audit._structure import _instruction_issues

    assert remap_pauli("-Y_0 Y_1", {0: 2, 1: 5}) == -Pauli("Y_2 Y_5")
    instruction = qc.Instruction(
        "signed_measure",
        inputs=[qc.instructions.BlockOperand("pair")],
        action=[qc.actions.Observe(["-Y_0 Y_1"])],
    )
    assert list(_instruction_issues(instruction, {"pair": 2})) == []


def test_clifford_channel_keeps_all_logical_generator_images() -> None:
    operand = qc.instructions.BlockOperand("pair")
    instruction = qc.Instruction(
        "green",
        inputs=[operand],
        outputs=[operand],
        action=[qc.actions.Clifford({"Z_0": "Y_0 X_1", "Z_1": "X_0 Z_1"})],
    )
    instruction_set = qc.InstructionSet(
        "logical", blocks=[qc.instructions.Block("pair", 2)], instructions=[instruction]
    )
    code = qc.Code("pair", [], ["X_0", "X_1"], ["Z_0", "Z_1"])
    encoding = Encoding(code, support=["0"], block_types=["pair"])
    gadget = qc.Gadget(
        instruction,
        qc.gadgets.Circuit(instruction_set, "- green: [0]", format="yaml"),
        inputs=[encoding],
        outputs=[encoding],
    )
    expected = {Pauli("X_0"), Pauli("Z_0"), Pauli("X_1"), Pauli("Z_1")}
    declared, realized = declared_action_of(gadget), realized_action_of(gadget)
    assert set(declared._mapping) == expected
    assert set(realized._mapping) == expected
    assert declared.is_equivalent_to(realized)


def test_c4_measurement_signs_use_circuit_readout_positions() -> None:
    code = qc.Code(
        "C4",
        stabilizers=["X_0 X_1 X_2 X_3", "Z_0 Z_1 Z_2 Z_3"],
        x=["X_0 X_1", "X_0 X_2"],
        z=["Z_0 Z_2", "Z_0 Z_1"],
    )
    gadget = (
        ec.build_qodec(code, strategy="bare-css/v1", strict=False)
        .layers[0]
        .gadgets["measure_z_all"]
    )
    profile = ec.GadgetProfile(gadget)
    assert {
        generator.pauli: generator.frame
        for generator in profile.action._observables.generators
    } == {Pauli("Z_0"): frozenset({0, 2}), Pauli("Z_1"): frozenset({0, 1})}


@pytest.mark.parametrize("decoded", [False, True])
def test_action_frames_use_circuit_readouts(
    measure_zz_gadget: qc.Gadget,
    idle_gadget: qc.Gadget,
    prepare_xx_gadget: qc.Gadget,
    decoded: bool,
) -> None:
    for gadget in (measure_zz_gadget, idle_gadget, prepare_xx_gadget):
        program = program_of(gadget)
        if decoded:
            action = realized_action_of(gadget)
        else:
            action = action_of(program)
        generators = (
            *action._stabilizers.generators,
            *action._observables.generators,
            *action._mapping.values(),
        )
        readouts = set(range(len(program.readouts)))
        assert all(generator.frame <= readouts for generator in generators)


def _action_from_stim(source: str) -> ChannelAction:
    return action_of(qc.gadgets.Circuit(_physical_isa(), source, format="stim"))


def test_interleaved_resets_do_not_offset_readout_indices() -> None:
    action = _action_from_stim("R 2\nM 0\nR 2\nM 1\n")
    assert {
        generator.pauli: generator.frame
        for generator in action._observables.generators
        if generator.pauli.weight
    } == {Pauli("Z_0"): frozenset({0}), Pauli("Z_1"): frozenset({1})}


def test_correlated_readouts_keep_one_shared_sign_variable() -> None:
    action = _action_from_stim("R 0 1\nH 0\nCX 0 1\nM 0\nM 1\n")
    assert action._stabilizers.frame_of(Pauli("Z_0")) == frozenset({0})
    assert action._stabilizers.frame_of(Pauli("Z_1")) == frozenset({0})
    assert action._stabilizers.frame_of(Pauli("Z_0 Z_1")) == frozenset()


def test_unrecorded_reset_outcome_is_averaged_out() -> None:
    action = _action_from_stim("R 0 1\nH 0\nCX 0 1\nR 0\n")
    assert (
        action._stabilizers.unframed == FrameGroup([PauliFrame(Pauli("Z_0"))]).unframed
    )
    assert all(not generator.frame for generator in action._stabilizers.generators)


def test_hidden_outcomes_cancel_in_retained_joint_relations() -> None:
    action = _action_from_stim("R 0 1 2\nH 0\nCX 0 1\nCX 0 2\nR 0\n")
    expected = FrameGroup([PauliFrame(Pauli("Z_0")), PauliFrame(Pauli("Z_1 Z_2"))])
    assert action._stabilizers.unframed == expected.unframed
    assert all(not generator.frame for generator in action._stabilizers.generators)


def test_readout_can_recover_a_hidden_reset_sign() -> None:
    action = _action_from_stim("R 0 1\nH 0\nCX 0 1\nR 0\nM 1\n")
    assert action._stabilizers.frame_of(Pauli("Z_0")) == frozenset()
    assert action._stabilizers.frame_of(Pauli("Z_1")) == frozenset({0})


def test_deterministic_readouts_keep_their_record_positions() -> None:
    action = _action_from_stim("R 1\nM 1\nM 0\n")
    assert action._observables.frame_of(Pauli("Z_0")) == frozenset({1})
    assert action._stabilizers.frame_of(Pauli("Z_1")) == frozenset()


def test_flags_keep_their_positions_in_action_readout_indices() -> None:
    instruction_set = _physical_isa()
    instruction_set.instructions["flag"] = qc.Instruction("flag", flags=["reject"])
    program = qc.gadgets.Circuit(instruction_set, "- flag: []\n- M: [0]", format="yaml")
    assert len(program.readouts) == 2
    assert action_of(program)._observables.frame_of(Pauli("Z_0")) == frozenset({1})


@pytest.mark.parametrize(
    "source",
    [
        "- negative_z: [0]",
        "- R: [0, 1]\n- H: [0]\n- CX: [0, 1]\n- R: [0]\n- negative_z: [1]",
    ],
)
def test_signed_readout_conversion_preserves_the_eigenvalue(source: str) -> None:
    instruction_set = _physical_isa()
    instruction_set.instructions["negative_z"] = qc.Instruction(
        "negative_z",
        inputs=[qc.instructions.BlockOperand("qubit")],
        action=[qc.actions.Observe(["-Z_0"])],
    )
    action = action_of(qc.gadgets.Circuit(instruction_set, source, format="yaml"))
    expected = PauliFrame(Pauli("-Z_1" if "CX" in source else "-Z_0"), frozenset({0}))
    assert expected in action._stabilizers.generators


def test_fault_probe_frames_are_indexed_by_readouts_not_reset_rows() -> None:
    program = qc.gadgets.Circuit(_physical_isa(), "R 1\nM 0\nR 1\n", format="stim")
    deltas, hidden, readouts = propagate_faults(
        program,
        [ec.FaultEvent.after(1, readout_flips=0)],
        [Pauli.identity()],
        residual_frames=[frozenset({0})],
    )
    assert hidden == 2 and readouts == 1
    assert deltas[hidden, 0]
    assert deltas[hidden + readouts, 0]
    with pytest.raises(ValueError, match="probe readout index 1 is out of bounds"):
        propagate_faults(
            program,
            [ec.FaultEvent()],
            [Pauli.identity()],
            residual_frames=[frozenset({1})],
        )


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


def test_action_comparison_does_not_assume_mapping_corrections() -> None:
    operator = Pauli("X_0")
    expected = ChannelAction._create(
        FrameGroup([]), FrameGroup([]), {operator: PauliFrame(operator)}
    )
    actual = ChannelAction._create(
        FrameGroup([]),
        FrameGroup([]),
        {operator: PauliFrame(-operator, frozenset({7}))},
    )
    assert not expected.is_equivalent_to(actual)
    assert expected.why_not_equivalent_to(actual)


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
    measurement, so an X-basis preparation (``P`` anticommutes with the |0⟩
    reset) left the prepared sign riding on the random projection outcome — a
    spurious frame on the *declared* action that made every prepare_x gadget mismatch
    its deterministic (reset + H) circuit. Z-basis preparations were
    unaffected because Z already stabilises |0⟩. Both must come out frame-free
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

    expected = (
        "mapping:\n" "  X_0 → X_0\n" "  Z_0 → Z_0\n" "  X_1 → X_1\n" "  Z_1 → Z_1"
    )
    assert str(profile.objective) == expected
    assert str(profile.action) == expected


@pytest.mark.parametrize(
    "mnemonic, expected",
    [
        ("prepare_zz", "stabilizers:\n  Z_0 = +1\n  Z_1 = +1"),
        (
            "transversal_cx",
            "mapping:\n"
            "  X_0 → X_0 X_2\n"
            "  Z_0 → Z_0\n"
            "  X_1 → X_1 X_3\n"
            "  Z_1 → Z_1\n"
            "  X_2 → X_2\n"
            "  Z_2 → Z_0 Z_2\n"
            "  X_3 → X_3\n"
            "  Z_3 → Z_1 Z_3",
        ),
        (
            "measure_zz",
            "observables:\n"
            "  Z_0 = -1^(circuit.readouts[0] ⊕ circuit.readouts[2])\n"
            "  Z_1 = -1^(circuit.readouts[0] ⊕ circuit.readouts[1])",
        ),
    ],
)
def test_action_display_examples(
    translation: qc.Layer, mnemonic: str, expected: str
) -> None:
    from IPython.lib.pretty import pretty

    action = ec.GadgetProfile(translation.gadgets[mnemonic]).action
    assert str(action) == expected
    assert repr(action) == expected
    assert pretty(action) == expected


@pytest.mark.parametrize(
    "operator, readouts, expected",
    [
        ("Z_1", (), "+1"),
        ("-Z_1", (), "-1"),
        ("Z_1", (0,), "-1^(circuit.readouts[0])"),
        ("-Z_1", (0,), "-1^(1 ⊕ circuit.readouts[0])"),
        ("Z_1", (12, 2), "-1^(circuit.readouts[2] ⊕ circuit.readouts[12])"),
        ("-Z_1", (12, 2), "-1^(1 ⊕ circuit.readouts[2] ⊕ circuit.readouts[12])"),
    ],
)
def test_action_display_preserves_eigenvalue_signs(
    operator: str, readouts: tuple[int, ...], expected: str
) -> None:
    action = ChannelAction._create(
        FrameGroup([]),
        FrameGroup([PauliFrame(Pauli(operator), frozenset(readouts))]),
        {},
    )
    assert str(action) == f"stabilizers:\n  Z_1 = {expected}"


@pytest.mark.parametrize(
    "operator, readouts, expected",
    [
        ("-Z_1", (), "-Z_1"),
        ("X_1", (1, 0), "-1^(circuit.readouts[0] ⊕ circuit.readouts[1]) X_1"),
        ("-Z_1", (0,), "-1^(1 ⊕ circuit.readouts[0]) Z_1"),
        ("iZ_1", (0,), "i -1^(circuit.readouts[0]) Z_1"),
        ("-iZ_1", (0,), "i -1^(1 ⊕ circuit.readouts[0]) Z_1"),
    ],
)
def test_action_display_preserves_mapping_signs(
    operator: str, readouts: tuple[int, ...], expected: str
) -> None:
    action = ChannelAction._create(
        FrameGroup([]),
        FrameGroup([]),
        {Pauli("X_1"): PauliFrame(Pauli(operator), frozenset(readouts))},
    )
    assert str(action) == f"mapping:\n  X_1 → {expected}"


def test_action_display_keeps_shared_readout_signs_across_sections() -> None:
    measured = PauliFrame(Pauli("Z_0"), frozenset({0}))
    action = ChannelAction._create(FrameGroup([measured]), FrameGroup([measured]), {})
    assert str(action) == (
        "observables:\n  Z_0 = -1^(circuit.readouts[0])\n"
        "stabilizers:\n  Z_0 = -1^(circuit.readouts[0])"
    )
    assert str(measured) == "Z^{0}"


def test_action_display_order_is_numeric_and_does_not_mutate_generators() -> None:
    operators = [Pauli("Z_12"), Pauli("-Z_2"), Pauli("X_2")]
    generators = tuple(PauliFrame(operator) for operator in operators)
    action = ChannelAction._create(FrameGroup([]), FrameGroup(generators), {})
    assert str(action) == "stabilizers:\n  X_2 = +1\n  Z_2 = -1\n  Z_12 = +1"
    assert action._stabilizers.generators == generators


def test_action_display_handles_empty_relations_and_pretty_cycles() -> None:
    from io import StringIO
    from IPython.lib.pretty import RepresentationPrinter, pretty

    action = ChannelAction._create(FrameGroup([]), FrameGroup([]), {})
    assert str(action) == "no observable, stabilizer, or mapping relations"
    assert repr(action) == pretty(action) == str(action)
    output = StringIO()
    printer = RepresentationPrinter(output)
    action._repr_pretty_(printer, cycle=True)
    printer.flush()
    assert output.getvalue() == "..."


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
    (call,) = declared_program_of(idle_gadget).calls()

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
    assert declared.calls()[0].arguments == {"theta": "theta"}


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
