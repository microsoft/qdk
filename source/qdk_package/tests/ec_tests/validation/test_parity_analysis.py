from __future__ import annotations

from pathlib import Path
from typing import Literal

import pytest
import qodec as qc
import qdk.ec as ec

from qdk.ec._audit._parity import ParityAnalysis
from qdk.ec._analysis.propagation.interpreter import _FramePropagator, walk_program
from qdk.ec._analysis.propagation.pauli import Pauli


def _feedforward_instruction_set(
    invert: bool, flagged: bool = False
) -> qc.InstructionSet:
    operand = qc.instructions.BlockOperand("qubit")
    return qc.InstructionSet(
        "physical",
        blocks=[qc.instructions.Block("qubit", 1)],
        instructions=[
            qc.Instruction(
                "R", outputs=[operand], action=[qc.actions.Stabilize(["Z_0"])]
            ),
            qc.Instruction(
                "H",
                inputs=[operand],
                outputs=[operand],
                action=[qc.actions.Clifford({"X_0": "Z_0", "Z_0": "X_0"})],
            ),
            qc.Instruction(
                "M",
                inputs=[operand],
                flags=["flag"] if flagged else [],
                action=[qc.actions.Observe(["Z_0"])],
            ),
            qc.Instruction(
                "correct",
                inputs=[operand],
                outputs=[operand],
                parameters=[qc.instructions.Parameter("bit", "bit")],
                action=[
                    qc.actions.Pauli(
                        "X_0", condition=qc.actions.Condition(["bit"], invert=invert)
                    )
                ],
            ),
        ],
    )


@pytest.mark.parametrize(
    "readout",
    ["circuit.readouts[0]", "circuit.readouts[00:01]", "circuit.readouts[0:2:2]"],
)
def test_walker_honors_measurement_conditioned_pauli(readout: str) -> None:
    for invert in (False, True):
        physical = _feedforward_instruction_set(invert)
        circuit = qc.gadgets.Circuit(
            physical,
            f'- R: [0]\n- H: [0]\n- M: [0]\n- correct: [0, bit: "{readout}"]',
            format="yaml",
        )
        assert circuit.calls()[-1].arguments["bit"] == "circuit.readouts[0]"
        assert readout in circuit.source
        frames = _FramePropagator(1)
        frames.apply_pauli_to_shot(0, Pauli("X_0"))
        simulation = walk_program(circuit, extra_engines=[frames]).simulation
        assert simulation.is_stabilizer(Pauli("Z_0"), ignore_sign=True)
        row = simulation.measure(Pauli("Z_0"))
        assert not any(
            simulation.outcome_matrix[row, index]
            for index in range(simulation.outcome_matrix.column_count)
        )
        assert bool(simulation.outcome_shift[row]) is invert
        frame_row = frames.measure(Pauli("Z_0"))
        assert not frames.outcome_deltas[frame_row, 0]


def test_feedforward_record_indices_include_preceding_flags() -> None:
    physical = _feedforward_instruction_set(False, flagged=True)
    circuit = qc.gadgets.Circuit(
        physical,
        '- R: [0]\n- H: [0]\n- M: [0]\n- R: [0]\n- M: [0]\n- correct: [0, bit: "circuit.readouts[2]"]',
        format="yaml",
    )
    simulation = walk_program(circuit).simulation
    row = simulation.measure(Pauli("Z_0"))
    assert not simulation.outcome_shift[row]
    assert not any(
        simulation.outcome_matrix[row, index]
        for index in range(simulation.outcome_matrix.column_count)
    )


def _c4() -> qc.Qodec:
    return qc.Qodec.load(
        str(Path(__file__).parents[1] / "testing/qodecs/c4.qodec.yaml")
    )


def _framed_preparation() -> qc.Gadget:
    operand = qc.instructions.BlockOperand("qubit")
    instruction = qc.Instruction(
        "prepare", outputs=[operand], action=[qc.actions.Stabilize(["Z_0"])]
    )
    encoding = qc.gadgets.Encoding(
        qc.Code("qubit", [], ["X_0"], ["Z_0"]), support=["0"]
    )
    return qc.Gadget(
        instruction,
        qc.gadgets.Circuit(
            _c4().layers[1].instruction_set, "R 0\nR 1\nH 0\nCX 0 1\nM 1", format="stim"
        ),
        outputs=[encoding],
        frames={"out[0].z[0]": ["circuit.readouts[0]"]},
    )


def test_declared_frame_makes_measurement_based_preparation_match() -> None:
    profile = ec.GadgetProfile(_framed_preparation())
    assert profile.objective is not None
    assert profile.action.is_equivalent_to(profile.objective)


def test_missing_or_wrong_frame_does_not_fix_preparation() -> None:
    gadget = _framed_preparation()
    for frames in ({}, {"out[0].z[0]": []}, {"out[0].x[0]": ["circuit.readouts[0]"]}):
        gadget.frames = frames
        profile = ec.GadgetProfile(gadget)
        assert profile.objective is not None
        assert not profile.action.is_equivalent_to(profile.objective)


def test_recorded_frame_bit_fault_changes_logical_output() -> None:
    profile = ec.GadgetProfile(_framed_preparation())
    fault = ec.FaultEvent.after(4, readout_flips=0)
    (effect,) = profile.effects_of([fault])
    assert effect == ec.FaultEffect(["out[0].z[0]"])
    assert profile.distance(faults=[fault]).value == 1


def test_constant_frame_flips_action_without_creating_faults() -> None:
    gadget = _framed_preparation()
    gadget.circuit.source = "R 0\nX 0"
    gadget.frames = {"out[0].z[0]": [1]}
    profile = ec.GadgetProfile(gadget)
    assert profile.objective is not None
    assert profile.action.is_equivalent_to(profile.objective)
    (effect,) = profile.effects_of([ec.FaultEvent()])
    assert not effect


@pytest.mark.parametrize(
    "frames, message",
    [
        ({"in[0].z[0]": []}, "output logical"),
        ({"out[0].stabilizers[0]": []}, "output logical"),
        ({"out[1].z[0]": []}, "out of bounds"),
        ({"out[0].z[1]": []}, "out of bounds"),
        ({"out[0].z[0]": ["out[0].z[0]"]}, "not available"),
        ({"out[0].z[0]": ["circuit.readouts[1]"]}, "out of bounds"),
    ],
)
def test_invalid_frames_are_rejected(
    frames: dict[str, list[str | Literal[0, 1]]], message: str
) -> None:
    gadget = _framed_preparation()
    gadget.frames = frames
    with pytest.raises(ValueError, match=message):
        _ = ec.GadgetProfile(gadget).action


def test_duplicate_frame_targets_are_rejected_on_assignment() -> None:
    gadget = _framed_preparation()
    with pytest.raises(ValueError, match="duplicate frame target"):
        gadget.frames = {"out[0].z[0]": [], "out[00].z[0]": []}


def test_frame_readout_cycles_are_rejected() -> None:
    gadget = _framed_preparation()
    gadget.readouts = [["readouts[0]"]]
    gadget.frames = {"out[0].z[0]": ["readouts[0]"]}
    with pytest.raises(ValueError, match="cyclic"):
        _ = ec.GadgetProfile(gadget).action


@pytest.mark.parametrize("field", ["x", "z", "stabilizers"])
@pytest.mark.parametrize("through_readout", [False, True])
def test_frames_reject_incoming_signs(field: str, through_readout: bool) -> None:
    from qdk.ec._audit import Auditor
    from qdk.ec._frames import FrameMap

    gadget = _conditional_pauli_gadget()
    reference = f"in[0].{field}[0]"
    gadget.readouts = [{"reject": [reference, reference]}]
    gadget.frames = {"out[0].z[0]": ["readouts[0]" if through_readout else reference]}
    with pytest.raises(
        ValueError, match="incoming signs are not allowed in frame deltas"
    ):
        FrameMap(gadget)
    report = Auditor().audit_gadget(gadget, qodec=_c4())
    assert any(item.rule == "qodec/invalid-structure" for item in report.errors)


def test_local_frame_delta_tracks_measurements_flipped_by_incoming_signs() -> None:
    gadget = _conditional_pauli_gadget()
    gadget.circuit.source = "- M: [1]"
    gadget.readouts = [{"reject": []}]
    relation = ("out[0].z[0]", "in[0].z[0]", "circuit.readouts[0]")
    gadget.checks = [relation]
    gadget.frames = {"out[0].z[0]": ["circuit.readouts[0]"]}
    analysis = ParityAnalysis(gadget)
    assert not analysis.value(("circuit.readouts[0]",)).is_zero
    assert analysis.value(relation).is_zero
    assert not analysis.value(relation[:2]).is_zero
    gadget.frames = {}
    assert ParityAnalysis(gadget).value(relation[:2]).is_zero


def test_frame_snapshot_and_readout_dependencies_preserve_constants() -> None:
    gadget = _framed_preparation()
    gadget.readouts = [["circuit.readouts[0]", 1]]
    gadget.frames = {"out[0].z[0]": ["readouts[0]", 1]}
    profile = ec.GadgetProfile(gadget)
    gadget.frames = {}
    assert profile.objective is not None
    assert profile.action.is_equivalent_to(profile.objective)


def test_physical_and_recorded_frame_faults_can_cancel() -> None:
    profile = ec.GadgetProfile(_framed_preparation())
    fault = ec.FaultEvent.after(4, Pauli("X_0"), readout_flips=0)
    (effect,) = profile.effects_of([fault])
    assert not effect
    quantum, recorded = profile.effects_of(
        [
            ec.FaultEvent.after(4, Pauli("X_0")),
            ec.FaultEvent.after(4, readout_flips=0),
        ]
    )
    assert quantum == recorded == ec.FaultEffect(["out[0].z[0]"])
    assert quantum ^ recorded == effect
    assert profile.distance(faults=[fault]).lower_bound is None


def test_completion_preserves_explicit_frames() -> None:
    gadget = _framed_preparation()
    completed = ec.filled(gadget)
    assert isinstance(completed, qc.Gadget)
    assert completed.frames == gadget.frames
    assert ec.GadgetProfile(completed).action.is_equivalent_to(
        ec.GadgetProfile(gadget).action
    )


def test_literal_parities_retain_affine_signs_and_cancel_pairs() -> None:
    gadget = _framed_preparation()
    gadget.checks = [[1], [1, 1], [0], ["circuit.readouts[0]", 1]]
    analysis = ParityAnalysis(gadget)
    assert not analysis.value(analysis.checks[0]).is_zero
    assert analysis.value(analysis.checks[1]).is_zero
    assert analysis.value(analysis.checks[2]).is_zero
    assert (
        analysis.value(analysis.checks[3]) ^ analysis.value(("circuit.readouts[0]",))
        == analysis.values["1"]
    )


def test_logical_preparation_is_independent_of_random_code_syndrome() -> None:
    gadget = _framed_preparation()
    gadget.frames = {}
    gadget.implements = qc.Instruction(
        gadget.implements.mnemonic,
        outputs=[qc.instructions.BlockOperand("qubit")],
        action=[qc.actions.Stabilize(["X_0"])],
    )
    gadget.circuit.source = "R 0\nH 0\nR 1\nH 1\nM 1\nH 1"
    gadget.outputs = [
        qc.gadgets.Encoding(
            qc.Code("pair", ["X_0 X_1"], ["X_0"], ["Z_0 Z_1"]), support=["0", "1"]
        )
    ]
    profile = ec.GadgetProfile(gadget)
    assert profile.objective is not None
    assert profile.action.is_equivalent_to(profile.objective)


def _conditional_pauli_gadget() -> qc.Gadget:
    physical = _feedforward_instruction_set(False)
    operand = qc.instructions.BlockOperand("data")
    instruction = qc.Instruction(
        "correct_ancilla", inputs=[operand], outputs=[operand], flags=["reject"]
    )
    code = qc.Code("data", ["Z_1"], ["X_0"], ["Z_0"])
    encoding = qc.gadgets.Encoding(code, support=["0", "1"])
    source = '- M: [1]\n- correct: [1, bit: "circuit.readouts[0]"]\n- M: [1]'
    return qc.Gadget(
        instruction,
        qc.gadgets.Circuit(physical, source, format="yaml"),
        inputs=[encoding],
        outputs=[encoding],
        checks=[["out[0].stabilizers[0]"]],
        readouts=[{"reject": ["circuit.readouts[1]"]}],
    )


def test_conditional_pauli_parities_include_incoming_frames() -> None:
    analysis = ParityAnalysis(_conditional_pauli_gadget())
    assert analysis.value(analysis.checks[0]).is_zero
    assert analysis.value(analysis.readouts[0]).is_zero
    assert not analysis.value(("circuit.readouts[0]",)).is_zero
    assert analysis.unresolved_outputs() == ()


def _measure_then_hadamard(*, measurement_first: bool) -> qc.Gadget:
    operand = qc.instructions.BlockOperand("qubit")
    observe = qc.actions.Observe(["Z_0"])
    hadamard = qc.actions.Clifford({"X_0": "Z_0", "Z_0": "X_0"})
    physical = _feedforward_instruction_set(False)
    instruction = qc.Instruction(
        "measure_h",
        inputs=[operand],
        outputs=[operand],
        action=[observe, hadamard] if measurement_first else [hadamard, observe],
    )
    encoding = qc.gadgets.Encoding(
        qc.Code("qubit", ["Z_1"], ["X_0"], ["Z_0"]), support=["0", "1"]
    )
    return qc.Gadget(
        instruction,
        qc.gadgets.Circuit(physical, "M 0\nH 0", format="stim"),
        inputs=[encoding],
        outputs=[encoding],
        readouts=[["circuit.readouts[0]", "in[0].z[0]"]],
        checks=[["out[0].stabilizers[0]", "in[0].stabilizers[0]"]],
    )


def test_leading_observation_can_precede_other_logical_actions() -> None:
    analysis = ParityAnalysis(_measure_then_hadamard(measurement_first=True))
    assert analysis.value(analysis.readouts[0]) == analysis.expected[0]
    assert analysis.value(("circuit.readouts[0]",)) != analysis.expected[0]


def test_observation_after_a_logical_transform_remains_unverified() -> None:
    analysis = ParityAnalysis(_measure_then_hadamard(measurement_first=False))
    with pytest.raises(NotImplementedError, match="interleaved logical actions"):
        _ = analysis.expected


def test_output_checks_do_not_require_logical_readout_verification() -> None:
    gadget = _measure_then_hadamard(measurement_first=False)
    assert ParityAnalysis(gadget).unresolved_outputs() == ()
    gadget.checks = [["out[0].stabilizers[0]"]]
    with pytest.raises(NotImplementedError, match="interleaved logical actions"):
        ParityAnalysis(gadget).unresolved_outputs()


def _rotation_gadget(axis: str) -> qc.Gadget:
    operand = qc.instructions.BlockOperand("qubit")
    rotation = qc.Instruction(
        "rotate",
        inputs=[operand],
        outputs=[operand],
        parameters=[qc.instructions.Parameter("theta", "number")],
        action=[qc.actions.Rotate(axis, "theta")],
    )
    physical = qc.InstructionSet(
        "physical", blocks=[qc.instructions.Block("qubit", 1)], instructions=[rotation]
    )
    encoding = qc.gadgets.Encoding(
        qc.Code("repetition", ["Z_0 Z_1"], ["X_0 X_1"], ["Z_0"]), support=["2", "5"]
    )
    return qc.Gadget(
        rotation,
        qc.gadgets.Circuit(physical, "- rotate: [2, theta: theta]", format="yaml"),
        inputs=[encoding],
        outputs=[encoding],
        checks=[["out[0].stabilizers[0]", "in[0].stabilizers[0]"]],
    )


def test_rotation_preserves_commuting_stabilizer_signs() -> None:
    analysis = ParityAnalysis(_rotation_gadget("Z_0"))
    assert analysis.value(analysis.checks[0]).is_zero
    assert not analysis.value(("out[0].stabilizers[0]",)).is_zero
    assert analysis.unresolved_outputs() == ()


def test_noncommuting_rotation_is_not_treated_as_identity() -> None:
    analysis = ParityAnalysis(_rotation_gadget("X_0"))
    with pytest.raises(TypeError, match="Rotate"):
        analysis.value(analysis.checks[0])


def test_rotation_invariants_do_not_certify_logical_frames() -> None:
    gadget = _rotation_gadget("Z_0")
    gadget.checks = [["out[0].x[0]", "in[0].x[0]"]]
    with pytest.raises(TypeError, match="Rotate"):
        ParityAnalysis(gadget).value(("out[0].x[0]", "in[0].x[0]"))


def _selected_flag_gadget(selection: str) -> qc.Gadget:
    physical = _feedforward_instruction_set(False, flagged=True)
    source = (
        "- R: [0]\n- H: [0]\n"
        f"- M: {{operands: [0], select: {selection}}}\n"
        '- correct: [0, bit: "circuit.readouts[0]"]\n- M: [0]'
    )
    return qc.Gadget(
        qc.Instruction("flag", flags=["reject"]),
        qc.gadgets.Circuit(physical, source, format="yaml"),
        readouts=[
            {
                "reject": [
                    "circuit.readouts[1]",
                    "circuit.readouts[2]",
                    "circuit.readouts[3]",
                ]
            }
        ],
    )


@pytest.mark.parametrize("selection", ["[{flag: 0}]", "[{flag: 1}, {flag: 0}]", "[{}]"])
def test_zero_flag_selection_retains_measurement_record_positions(
    selection: str,
) -> None:
    analysis = ParityAnalysis(_selected_flag_gadget(selection))
    assert analysis.value(analysis.readouts[0]).is_zero
    assert not analysis.value(("circuit.readouts[0]",)).is_zero
    assert all(
        analysis.value((f"circuit.readouts[{index}]",)).is_zero for index in (1, 2, 3)
    )


def test_selection_rejecting_noiseless_execution_stays_unverified() -> None:
    analysis = ParityAnalysis(_selected_flag_gadget("[{flag: 1}]"))
    with pytest.raises(
        NotImplementedError, match="rejects the noiseless zero-flag branch"
    ):
        analysis.value(analysis.readouts[0])


def test_selection_cannot_assume_an_unknown_flag_is_zero() -> None:
    analysis = ParityAnalysis(_selected_flag_gadget("[{unknown: 0}]"))
    with pytest.raises(ValueError, match="undeclared instruction flag"):
        analysis.value(analysis.readouts[0])


def test_missing_output_suggestions_include_preceding_flag_slots() -> None:
    from qdk.ec._audit.rules.gadget import IncompleteOutputFrameRule

    gadget = _selected_flag_gadget("[{flag: 0}]")
    gadget.circuit.source = "- R: [0]\n- M: [0]\n- H: [0]\n- M: [0]"
    gadget.outputs = [
        qc.gadgets.Encoding(qc.Code("measured", ["Z_0"], [], []), support=["0"])
    ]
    gadget.readouts = []
    diagnostics = list(IncompleteOutputFrameRule()(gadget, qodec=_c4()))
    assert len(diagnostics) == 1
    assert (
        'Verified relation: ["out[0].stabilizers[0]", "circuit.readouts[2]"]'
        in diagnostics[0].detail
    )


def test_missing_output_relations_do_not_claim_unsupported_analysis_succeeded() -> None:
    from qdk.ec._audit.rules.gadget import IncompleteOutputFrameRule

    protocol = _c4()
    gadget = protocol.layers[0].gadgets["transversal_cx"]
    gadget.checks = []
    gadget.circuit.format = "unknown"
    diagnostics = list(IncompleteOutputFrameRule()(gadget, qodec=protocol))
    assert len(diagnostics) == 1
    assert diagnostics[0].summary == "Output frames not checked: analysis failed"
    assert "Verified relation" not in diagnostics[0].detail


def test_readout_requires_incoming_logical_frame() -> None:
    gadget = _c4().layers[0].gadgets["measure_zz"]
    analysis = ParityAnalysis(gadget)
    assert not (analysis.value(analysis.readouts[0]) ^ analysis.expected[0]).is_zero
    candidate = analysis.candidate(analysis.expected[0])
    assert candidate is not None
    assert "in[0].z[0]" in candidate
    assert (analysis.value(candidate) ^ analysis.expected[0]).is_zero


def test_check_validity_and_tautological_cancellation() -> None:
    gadget = _c4().layers[0].gadgets["measure_zz"]
    analysis = ParityAnalysis(gadget)
    assert all(analysis.value(equation).is_zero for equation in analysis.checks)
    assert not analysis.value(("circuit.readouts[0]",)).is_zero
    gadget.checks = [["circuit.readouts[0]", "circuit.readouts[0]"]]
    assert ParityAnalysis(gadget).checks == ((),)
    gadget.checks = [["circuit.readouts[00]", "circuit.readouts[0:1]"]]
    assert ParityAnalysis(gadget).checks == ((),)


def test_readout_dependencies_are_solved() -> None:
    gadget = _c4().layers[0].gadgets["measure_zz"]
    gadget.readouts = [["readouts[1]"], ["circuit.readouts[0]"]]
    analysis = ParityAnalysis(gadget)
    resolution = analysis.resolution
    assert not resolution.conflicts and not resolution.unresolved
    assert (
        resolution.values[0]
        == resolution.values[1]
        == analysis.value(("circuit.readouts[0]",))
    )
    gadget.readouts = [["readouts[0]"]]
    assert ParityAnalysis(gadget).resolution.unresolved == {0}
    gadget.readouts = [["readouts[0]", "circuit.readouts[0]"]]
    assert ParityAnalysis(gadget).resolution.conflicts == ((0,),)


def test_readout_conflict_blocks_only_dependent_equations() -> None:
    gadget = _c4().layers[0].gadgets["measure_zz"]
    gadget.implements.flags = ["reject"]
    gadget.readouts = [
        ["readouts[0]", "circuit.readouts[0]"],
        ["circuit.readouts[1]"],
        ["readouts[0]"],
    ]
    analysis = ParityAnalysis(gadget)
    resolution = analysis.resolution
    assert resolution.conflicts == ((0,),)
    assert resolution.blocked == {0, 2}
    assert resolution.unresolved == set()
    assert resolution.values == {1: analysis.value(("circuit.readouts[1]",))}


def test_readout_cycle_is_reported_as_one_conflict() -> None:
    gadget = _c4().layers[0].gadgets["measure_zz"]
    gadget.readouts = [["readouts[1]"], ["readouts[0]", 1]]
    resolution = ParityAnalysis(gadget).resolution
    assert resolution.conflicts == ((0, 1),)
    assert resolution.blocked == {0, 1}
    assert resolution.values == {}


def test_output_frames_require_valid_independent_constraints() -> None:
    gadget = _c4().layers[0].gadgets["transversal_cx"]
    signs = [
        f"out[{entry}].stabilizers[{index}]" for entry in range(2) for index in range(2)
    ]
    gadget.checks = [[sign, sign] for sign in signs]
    assert ParityAnalysis(gadget).unresolved_outputs() == tuple(signs)
    gadget.checks = [
        [signs[0], "in[0].stabilizers[0]", "in[1].stabilizers[0]"],
        [signs[1], "in[0].stabilizers[1]"],
        [signs[2], "in[1].stabilizers[0]"],
        [signs[3], "in[0].stabilizers[1]", "in[1].stabilizers[1]"],
    ]
    assert ParityAnalysis(gadget).unresolved_outputs() == ()
    gadget.checks = [
        [
            signs[0],
            signs[1],
            "in[0].stabilizers[0]",
            "in[1].stabilizers[0]",
            "in[0].stabilizers[1]",
        ]
    ]
    assert ParityAnalysis(gadget).unresolved_outputs() == tuple(signs)


def test_readout_sign_mismatch_and_dependency_errors_are_reported() -> None:
    protocol = _c4()
    gadget = protocol.layers[0].gadgets["measure_zz"]
    for readouts, expected in (
        (
            [["circuit.readouts[0]", "circuit.readouts[2]", "in[0].x[0]"]],
            "Verified readout equation:",
        ),
        ([["readouts[0]"]], "not uniquely determined"),
        ([["readouts[0]", "circuit.readouts[0]"]], "cannot all hold"),
    ):
        gadget.readouts = [
            *readouts,
            ["circuit.readouts[0]", "circuit.readouts[1]", "in[0].z[1]"],
        ]
        errors = [
            item
            for item in ec.audit(protocol).errors
            if item.rule == "gadget/readout-mismatch"
            and "gadgets['measure_zz']" in item.where
        ]
        assert errors
        assert expected in errors[0].detail


def test_invalid_check_has_concrete_witness() -> None:
    protocol = _c4()
    gadget = protocol.layers[0].gadgets["measure_zz"]
    gadget.checks = [["circuit.readouts[0]"]]
    diagnostic = next(
        item
        for item in ec.audit(protocol).errors
        if item.rule == "gadget/check-mismatch"
    )
    assert 'Declared equation: ["circuit.readouts[0]"]' in diagnostic.detail
    assert 'Equation term values: {"circuit.readouts[0]": 1}' in diagnostic.detail
    assert "can fire without a fault" in diagnostic.summary
    assert (
        "Declared equation produces: 1\nRequired noiseless value: 0"
        in diagnostic.detail
    )


def test_always_one_flag_fails_and_zero_flag_passes() -> None:
    operand = qc.instructions.BlockOperand("qubit")
    physical = qc.InstructionSet(
        "physical",
        blocks=[qc.instructions.Block("qubit", encodes=1)],
        instructions=[
            qc.Instruction(
                "R", outputs=[operand], action=[qc.actions.Stabilize(["Z_0"])]
            ),
            qc.Instruction(
                "X",
                inputs=[operand],
                outputs=[operand],
                action=[qc.actions.Pauli("X_0")],
            ),
            qc.Instruction("M", inputs=[operand], action=[qc.actions.Observe(["Z_0"])]),
        ],
    )
    instruction = qc.Instruction("flag_only", flags=["reject"])
    for source, expected_errors in (("R 0\nX 0\nM 0", 1), ("R 0\nM 0", 0)):
        gadget = qc.Gadget(
            instruction,
            qc.gadgets.Circuit(physical, source, format="stim"),
            readouts=[{"reject": ["circuit.readouts[0]"]}],
        )
        protocol = qc.Qodec(
            [
                qc.Layer(
                    qc.InstructionSet("logical", instructions=[instruction]),
                    gadgets=[gadget],
                ),
                qc.Layer(physical),
            ]
        )
        errors = [
            item
            for item in ec.audit(protocol).errors
            if item.rule == "gadget/flag-mismatch"
        ]
        assert len(errors) == expected_errors
        if errors:
            assert "always fires, even without a fault" in errors[0].summary
            assert (
                "Declared equation always produces: 1\nRequired noiseless value: 0"
                in errors[0].detail
            )


def test_solvable_readout_cycle_is_not_rejected() -> None:
    gadget = _c4().layers[0].gadgets["measure_zz"]
    gadget.readouts = [
        ["readouts[1]", "readouts[2]", "circuit.readouts[0]"],
        ["readouts[0]", "circuit.readouts[1]"],
        ["readouts[1]", "circuit.readouts[2]"],
    ]
    analysis = ParityAnalysis(gadget)
    resolution = analysis.resolution
    assert not resolution.unresolved and not resolution.conflicts
    assert resolution.values[0] == analysis.value(
        ("circuit.readouts[0]", "circuit.readouts[2]")
    )
    assert resolution.values[2] == analysis.value(
        ("circuit.readouts[0]", "circuit.readouts[1]")
    )


def test_output_logical_frame_tracks_the_declared_operation() -> None:
    gadget = _c4().layers[0].gadgets["x0"]
    gadget.checks = [
        [f"in[0].{basis}[{index}]", f"out[0].{basis}[{index}]"]
        for basis in ("x", "z")
        for index in (0, 1)
    ]
    analysis = ParityAnalysis(gadget)
    assert all(analysis.value(check).is_zero for check in analysis.checks)
    gadget.checks = [["in[0].x[0]", "out[0].z[0]"]]
    changed = ParityAnalysis(gadget)
    assert not changed.value(changed.checks[0]).is_zero


def test_reset_between_measurements_erases_the_previous_outcome() -> None:
    physical = _c4().layers[1].instruction_set
    instruction = qc.Instruction("reset_then_flag", flags=["reject"])
    gadget = qc.Gadget(
        instruction,
        qc.gadgets.Circuit(physical, "R 0\nH 0\nM 0\nR 0\nM 0", format="stim"),
        readouts=[{"reject": ["circuit.readouts[1]"]}],
    )
    analysis = ParityAnalysis(gadget)
    assert analysis.value(("circuit.readouts[1]",)).is_zero
    assert not analysis.value(("circuit.readouts[0]",)).is_zero


def test_reset_of_known_input_stabilizer_removes_incoming_frame() -> None:
    physical = _c4().layers[1].instruction_set
    operand = qc.instructions.BlockOperand("data")
    instruction = qc.Instruction("reset_ancilla", inputs=[operand], outputs=[operand])
    code = qc.Code("data-with-ancilla", stabilizers=["Z_1"], x=["X_0"], z=["Z_0"])
    encoding = qc.gadgets.Encoding(code, support=["0", "1"])
    gadget = qc.Gadget(
        instruction,
        qc.gadgets.Circuit(physical, "R 1", format="stim"),
        inputs=[encoding],
        outputs=[encoding],
        checks=[["out[0].stabilizers[0]"]],
    )
    analysis = ParityAnalysis(gadget)
    assert analysis.value(analysis.checks[0]).is_zero


def test_random_flag_and_constant_one_check_are_errors() -> None:
    physical = _c4().layers[1].instruction_set
    instruction = qc.Instruction("flag", flags=["reject"])
    for source, rule in (
        ("R 0\nH 0\nM 0", "gadget/flag-mismatch"),
        ("R 0\nX 0\nM 0", "gadget/check-mismatch"),
    ):
        gadget = qc.Gadget(
            instruction,
            qc.gadgets.Circuit(physical, source, format="stim"),
            checks=[["circuit.readouts[0]"]],
            readouts=[{"reject": ["circuit.readouts[0]"]}],
        )
        protocol = qc.Qodec(
            [
                qc.Layer(
                    qc.InstructionSet("logical", instructions=[instruction]),
                    gadgets=[gadget],
                ),
                qc.Layer(physical),
            ]
        )
        diagnostic = next(
            item for item in ec.audit(protocol).errors if item.rule == rule
        )
        assert "Declared equation" in diagnostic.detail
        assert "Required noiseless value: 0" in diagnostic.detail


def test_logical_readout_reference_in_check_is_not_dropped() -> None:
    protocol = _c4()
    gadget = protocol.layers[0].gadgets["measure_zz"]
    gadget.readouts = [
        ["circuit.readouts[0]", "circuit.readouts[2]", "in[0].z[0]"],
        ["circuit.readouts[0]", "circuit.readouts[1]", "in[0].z[1]"],
    ]
    gadget.checks = [["readouts[0]"]]
    errors = [
        item
        for item in ec.audit(protocol).errors
        if "gadgets['measure_zz']" in item.where
    ]
    assert {item.rule for item in errors} == {"gadget/check-mismatch"}


def test_empty_equations_are_zero_even_for_opaque_circuit() -> None:
    instruction = qc.Instruction("flag", flags=["reject"])
    physical = _c4().layers[1].instruction_set
    gadget = qc.Gadget(
        instruction,
        qc.gadgets.Circuit(physical, "opaque source", format="openqasm"),
        checks=[[]],
        readouts=[{"reject": []}],
    )
    protocol = qc.Qodec(
        [
            qc.Layer(
                qc.InstructionSet("logical", instructions=[instruction]),
                gadgets=[gadget],
            ),
            qc.Layer(physical),
        ]
    )
    assert not [
        item
        for item in ec.audit(protocol).diagnostics
        if item.rule in {"gadget/check-mismatch", "gadget/flag-mismatch"}
    ]


def test_unsupported_circuit_yields_unverified_parity_warning() -> None:
    from qdk.ec._audit.rules.gadget import CheckMismatchRule

    protocol = _c4()
    gadget = protocol.layers[0].gadgets["idle"]
    gadget.circuit = qc.gadgets.Circuit(
        gadget.circuit.instruction_set, "opaque source", format="openqasm"
    )
    diagnostic = next(iter(CheckMismatchRule()(gadget, qodec=protocol)))
    assert diagnostic.severity is ec.Diagnostic.Severity.WARNING
    assert "not checked" in diagnostic.summary


def test_y_readout_preserves_complex_conjugation_sign() -> None:
    operand = qc.instructions.BlockOperand("qubit")
    instruction = qc.Instruction(
        "measure_y", inputs=[operand], action=[qc.actions.Observe(["Y_0"])]
    )
    physical = qc.InstructionSet(
        "physical",
        blocks=[qc.instructions.Block("qubit", encodes=1)],
        instructions=[
            qc.Instruction("MY", inputs=[operand], action=[qc.actions.Observe(["Y_0"])])
        ],
    )
    encoding = qc.gadgets.Encoding(
        qc.Code("qubit", stabilizers=[], x=["X_0"], z=["Z_0"]), support=["0"]
    )
    gadget = qc.Gadget(
        instruction,
        qc.gadgets.Circuit(physical, "- MY: [0]", format="yaml"),
        inputs=[encoding],
        readouts=[["circuit.readouts[0]", "in[0].x[0]", "in[0].z[0]"]],
    )
    analysis = ParityAnalysis(gadget)
    assert analysis.value(analysis.readouts[0]) == analysis.expected[0]


def test_composite_logical_observable_retains_product_phase() -> None:
    encoding = _c4().layers[0].gadgets["measure_zz"].inputs[0]
    operand = qc.instructions.BlockOperand("qubit")
    physical = qc.InstructionSet(
        "physical",
        blocks=[qc.instructions.Block("qubit", encodes=1)],
        instructions=[
            qc.Instruction("MY", inputs=[operand], action=[qc.actions.Observe(["Y_0"])])
        ],
    )
    instruction = qc.Instruction(
        "measure_xz",
        inputs=[qc.instructions.BlockOperand("c4")],
        action=[qc.actions.Observe(["X_0 Z_1"])],
    )
    gadget = qc.Gadget(
        instruction,
        qc.gadgets.Circuit(physical, "- MY: [0]\n- MY: [1]", format="yaml"),
        inputs=[encoding],
        readouts=[
            ["circuit.readouts[0]", "circuit.readouts[1]", "in[0].x[0]", "in[0].z[1]"]
        ],
    )
    analysis = ParityAnalysis(gadget)
    difference = analysis.value(analysis.readouts[0]) ^ analysis.expected[0]
    assert not any(difference[index] for index in range(len(difference) - 1))
    assert difference[len(difference) - 1]


def test_constant_one_check_with_incoming_frames() -> None:
    protocol = _c4()
    gadget = protocol.layers[0].gadgets["x0"]
    gadget.circuit = qc.gadgets.Circuit(
        gadget.circuit.instruction_set, "X 0", format="stim"
    )
    gadget.checks = [["in[0].stabilizers[1]", "out[0].stabilizers[1]"]]
    diagnostic = next(
        item
        for item in ec.audit(protocol).errors
        if item.rule == "gadget/check-mismatch" and "gadgets['x0']" in item.where
    )
    assert (
        "Declared equation always produces: 1\nRequired noiseless value: 0"
        in diagnostic.detail
    )
