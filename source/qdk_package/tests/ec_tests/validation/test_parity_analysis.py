from __future__ import annotations

from pathlib import Path

import qodec as qc
import qdk.ec as ec

from qdk.ec._audit._parity import ParityAnalysis


def _c4() -> qc.Qodec:
    return qc.Qodec.load(
        str(Path(__file__).parents[1] / "testing/qodecs/c4.qodec.yaml")
    )


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
    values, unresolved, error = analysis.resolved
    assert error is None and not unresolved
    assert values[0] == values[1] == analysis.value(("circuit.readouts[0]",))
    gadget.readouts = [["readouts[0]"]]
    assert ParityAnalysis(gadget).resolved[1] == {0}
    gadget.readouts = [["readouts[0]", "circuit.readouts[0]"]]
    assert (
        ParityAnalysis(gadget).resolved[2]
        == "Inconsistent readout equations at positions [0]."
    )


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
        ([["readouts[0]", "circuit.readouts[0]"]], "Inconsistent readout equations"),
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
    values, unresolved, error = analysis.resolved
    assert not unresolved and error is None
    assert values[0] == analysis.value(("circuit.readouts[0]", "circuit.readouts[2]"))
    assert values[2] == analysis.value(("circuit.readouts[0]", "circuit.readouts[1]"))


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
