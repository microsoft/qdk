"""Declaration checks owned by audit rather than persistence."""

from __future__ import annotations

import json
from pathlib import Path

import pytest
import qodec as qc

from qdk.ec._audit import audit
from qdk.ec._audit._structure import _instruction_issues
from qdk.ec._audit._structure import structural_issues
from ec_tests.testing.optional import requires_stim


@pytest.mark.parametrize(
    "action,parameters,expected",
    [
        (qc.actions.Pauli("X_2"), [], "not been introduced"),
        (
            qc.actions.Pauli("X_0", condition=qc.actions.Condition(["outcomes[0]"])),
            [],
            "preceding outcomes",
        ),
        (
            qc.actions.Pauli("X_0", condition=qc.actions.Condition(["enabled"])),
            [],
            "bit parameter",
        ),
        (qc.actions.Rotate("Z_0", "theta"), [], "number or integer"),
        (
            qc.actions.Rotate("Z_0", "theta"),
            [qc.instructions.Parameter("theta", "string")],
            "number or integer",
        ),
    ],
)
def test_action_declaration_checks(
    action: qc.Action, parameters: list[qc.instructions.Parameter], expected: str
) -> None:
    instruction = qc.Instruction(
        "draft",
        inputs=[qc.instructions.BlockOperand("qubit")],
        parameters=parameters,
        action=[action],
    )
    assert any(
        expected in issue for issue in _instruction_issues(instruction, {"qubit": 1})
    )


def test_valid_temporary_and_pauli_parameter() -> None:
    instruction = qc.Instruction(
        "draft",
        inputs=[qc.instructions.BlockOperand("q")],
        parameters=[qc.instructions.Parameter("p", "pauli")],
        action=[
            qc.actions.Stabilize(["Z_3"]),
            qc.actions.Pauli("X_3"),
            qc.actions.Observe(["p"]),
        ],
    )
    assert list(_instruction_issues(instruction, {"q": 1})) == []


@requires_stim
def test_invalid_source_and_reference_are_audit_findings(rep3_qodec: qc.Qodec) -> None:
    gadget = rep3_qodec.layers[0].gadgets["idle"]
    gadget.circuit.source = "NOT_A_GATE 0"
    gadget.checks = [["in[9].x[0]"]]
    report = audit(rep3_qodec)
    errors = [item for item in report.errors if item.rule == "qodec/invalid-structure"]
    assert any("circuit:" in item.summary for item in errors)
    assert any("in[9].x[0]" in item.summary for item in errors)
    assert not [item for item in report.warnings if "gadgets['idle']" in item.where]


def test_registered_formats_get_all_call_structure_checks(rep3_qodec: qc.Qodec) -> None:
    gadget = rep3_qodec.layers[0].gadgets["idle"]
    qc.register(
        lambda source, target: [
            qc.instructions.InstructionCall(
                "M", operands=[0], arguments={"typo": 1}, select=[{"missing": 0}]
            )
        ],
        format="custom-structure-checks",
    )
    gadget.circuit.format = "custom-structure-checks"
    gadget.checks = [["circuit.readouts[1]"]]
    messages = [item.summary for item in audit(rep3_qodec).errors]
    assert any(
        "argument 'typo' is not declared" in message for message in messages
    ), messages
    assert any("unknown flag 'missing'" in message for message in messages), messages
    assert any(
        "index 1 is out of bounds for 1 entries" in message for message in messages
    ), messages


def test_registered_parser_failure_is_a_structure_finding(rep3_qodec: qc.Qodec) -> None:
    def fail(
        source: str, target: qc.InstructionSet
    ) -> list[qc.instructions.InstructionCall]:
        raise ValueError("invalid custom source")

    qc.register(fail, format="custom-invalid-source")
    gadget = rep3_qodec.layers[0].gadgets["idle"]
    gadget.circuit.format = "custom-invalid-source"
    messages = [message for _, message in structural_issues(gadget)]
    assert "circuit: invalid custom source" in messages


def test_missing_parser_does_not_claim_invalid_source(rep3_qodec: qc.Qodec) -> None:
    gadget = rep3_qodec.layers[0].gadgets["idle"]
    gadget.circuit.format = "unregistered-structure-review"
    with pytest.raises(ValueError, match="No source parser registered"):
        gadget.circuit.calls()
    assert list(structural_issues(gadget)) == []


def test_unequal_code_lists_round_trip_but_analysis_rejects(
    rep3_qodec: qc.Qodec,
) -> None:
    from qdk.ec import CodeProfile

    code = rep3_qodec.codes["repetition3"]
    code.z = []
    rep3_qodec.validate()
    reloaded = qc.Qodec.loads(rep3_qodec.dumps())
    assert reloaded.codes[code.name].z == []
    report = audit(reloaded)
    assert any("counts disagree" in item.summary for item in report.errors)
    with pytest.raises(ValueError, match="counts disagree"):
        CodeProfile(code)


@pytest.mark.parametrize(
    "format,source",
    [
        pytest.param("stim", "NOT_A_GATE 0\n", marks=requires_stim),
        ("yaml", "[not valid yaml"),
        ("custom", "arbitrary text"),
    ],
)
def test_source_text_round_trips_before_audit(
    rep3_qodec: qc.Qodec, format: str, source: str
) -> None:
    gadget = rep3_qodec.layers[0].gadgets["idle"]
    gadget.circuit.format = format
    gadget.circuit.source = source
    reloaded = qc.Qodec.loads(rep3_qodec.dumps())
    circuit = reloaded.layers[0].gadgets["idle"].circuit
    assert circuit.source == source
    assert circuit.format == format
    with pytest.raises(ValueError):
        _ = circuit.calls()
    report = audit(reloaded)
    if format != "custom":
        assert any("circuit:" in item.summary for item in report.errors)


def test_short_layer_lists_can_be_saved_but_audit_is_incomplete() -> None:
    for layers in ([], [qc.Layer(qc.InstructionSet("draft"))]):
        protocol = qc.Qodec(layers)
        protocol.validate()
        assert len(qc.Qodec.loads(protocol.dumps()).layers) == len(layers)
        assert any(
            "at least two layers" in item.summary for item in audit(protocol).errors
        )


@pytest.mark.parametrize("remove_gadgets", [False, True])
def test_encoded_layers_require_explicit_code_bindings(
    rep3_qodec: qc.Qodec, remove_gadgets: bool
) -> None:
    layer = rep3_qodec.layers[0]
    layer.codes.clear()
    if remove_gadgets:
        layer.gadgets.clear()
    else:
        assert list(structural_issues(layer.gadgets["idle"])) == []

    errors = [
        item
        for item in audit(rep3_qodec).errors
        if item.rule == "qodec/invalid-structure"
    ]
    assert len(errors) == 1
    assert errors[0].where.startswith("layers[0] (")
    assert errors[0].summary == (
        "block 'repetition3' has no code binding in the layer"
    )


def test_physical_layer_requires_no_code_bindings(rep3_qodec: qc.Qodec) -> None:
    assert dict(rep3_qodec.layers[-1].codes) == {}
    assert list(structural_issues(rep3_qodec)) == []


def test_explicit_code_bindings_must_match_every_gadget_encoding(
    rep3_qodec: qc.Qodec,
) -> None:
    layer = rep3_qodec.layers[0]
    code = layer.codes["repetition3"]
    layer.codes["repetition3"] = qc.Code(
        "other", stabilizers=code.stabilizers, x=code.x, z=code.z
    )

    paths = {
        path
        for path, message in structural_issues(rep3_qodec)
        if message == "block 'repetition3' is bound to different codes in the layer"
    }
    assert paths == {
        f"layers[0].gadgets[{json.dumps(mnemonic)}]" for mnemonic in layer.gadgets
    }
    assert any(
        item.rule == "qodec/invalid-structure"
        and "bound to different codes" in item.summary
        for item in audit(rep3_qodec).errors
    )


@pytest.mark.parametrize(
    "kind,value,expected",
    [
        ("number", "true", "expected number"),
        ("integer", "1.5", "expected integer"),
        ("boolean", "1", "expected boolean"),
        ("bit", "2", "expected bit"),
        ("bit", "'circuit.readouts[0]'", "preceding readouts"),
        ("number", "'circuit.readouts[0]'", "only a bit"),
        ("pauli", "'Q_0'", "expected pauli"),
        ("string", "[alpha, beta]", ""),
        ("integer", "[1, 2]", ""),
        ("number", "-2", ""),
        ("boolean", "false", ""),
    ],
)
def test_argument_types_are_audited_after_round_trip(
    kind: str, value: str, expected: str
) -> None:
    instruction = qc.Instruction(
        "probe", parameters=[qc.instructions.Parameter("value", kind)], flags=["reject"]
    )
    physical = qc.InstructionSet("physical", instructions=[instruction])
    logical = qc.InstructionSet("logical", instructions=[qc.Instruction("draft")])
    gadget = qc.Gadget(
        logical.instructions["draft"],
        qc.gadgets.Circuit(physical, f"- probe: [value: {value}]", format="yaml"),
    )
    protocol = qc.Qodec([qc.Layer(logical, gadgets=[gadget]), qc.Layer(physical)])
    saved = qc.Qodec.loads(protocol.dumps())
    issues = list(structural_issues(saved))
    if expected:
        assert any(expected in message for _, message in issues), issues
    else:
        assert issues == []


def test_forwarding_and_select_are_checked() -> None:
    physical = qc.InstructionSet(
        "physical",
        instructions=[
            qc.Instruction(
                "probe",
                parameters=[qc.instructions.Parameter("value", "number")],
                flags=["reject"],
            )
        ],
    )
    instruction = qc.Instruction(
        "draft", parameters=[qc.instructions.Parameter("theta", "string")]
    )
    gadget = qc.Gadget(
        instruction,
        qc.gadgets.Circuit(
            physical,
            "- probe: {arguments: {value: forwarded}, select: [{'flags[2]': 0}]}",
            format="yaml",
        ),
        parameter_bindings={"theta": "forwarded", "missing": "unused"},
    )
    messages = [message for _, message in structural_issues(gadget)]
    assert any("incompatible type" in message for message in messages), messages
    assert any("unknown flag" in message for message in messages), messages
    assert any("binding 'missing'" in message for message in messages), messages


def test_preceding_record_and_valid_select_are_accepted() -> None:
    instruction = qc.Instruction(
        "probe",
        parameters=[qc.instructions.Parameter("value", "bit")],
        flags=["reject"],
    )
    physical = qc.InstructionSet("physical", instructions=[instruction])
    gadget = qc.Gadget(
        qc.Instruction("draft"),
        qc.gadgets.Circuit(
            physical,
            "- probe: []\n- probe: {arguments: {value: 'circuit.readouts[0]'}, select: [{reject: 0}, {'flags[0]': 1}]}",
            format="yaml",
        ),
    )
    assert list(structural_issues(gadget)) == []


def test_partial_readouts_and_code_capacity_are_preserved_for_audit(
    rep3_qodec: qc.Qodec,
) -> None:
    source = rep3_qodec.layers[0]
    source.instruction_set.blocks = [qc.instructions.Block("repetition3", encodes=2)]
    gadget = source.gadgets["measure_z"]
    gadget.readouts = [[], []]
    saved = qc.Qodec.loads(rep3_qodec.dumps())
    messages = [message for _, message in structural_issues(saved)]
    assert any("requires 2" in message for message in messages), messages
    assert any(
        "2 entries supplied; expected 1" in message for message in messages
    ), messages


@pytest.mark.parametrize(
    "actions,expected",
    [
        (
            [
                qc.actions.Stabilize(
                    ["Z_3"], condition=qc.actions.Condition(["enabled"])
                )
            ],
            "not been introduced",
        ),
        (
            [qc.actions.Stabilize(["Z_3"]), qc.actions.Pauli("X_2")],
            "not been introduced",
        ),
        (
            [qc.actions.Stabilize(["I_3"]), qc.actions.Pauli("X_3")],
            "not been introduced",
        ),
        (
            [
                qc.actions.Observe(["Z_0"]),
                qc.actions.Pauli(
                    "X_0", condition=qc.actions.Condition(["outcomes[0]"])
                ),
            ],
            "",
        ),
    ],
)
def test_action_order_and_sparse_temporaries(
    actions: list[qc.Action], expected: str
) -> None:
    instruction = qc.Instruction(
        "draft",
        inputs=[qc.instructions.BlockOperand("q")],
        parameters=[qc.instructions.Parameter("enabled", "bit")],
        action=actions,
    )
    messages = list(_instruction_issues(instruction, {"q": 1}))
    if expected:
        assert any(expected in message for message in messages), messages
    else:
        assert messages == []


def test_attached_instruction_errors_are_not_repeated_on_gadgets(
    rep3_qodec: qc.Qodec,
) -> None:
    layer = rep3_qodec.layers[0]
    gadget = layer.gadgets["idle"]
    original = gadget.implements
    instruction = qc.Instruction(
        "idle",
        inputs=original.inputs,
        outputs=original.outputs,
        action=[qc.actions.Rotate("Z_0", "undeclared_angle")],
    )
    gadget.implements = instruction
    layer.instruction_set.instructions = {
        **layer.instruction_set.instructions,
        "idle": instruction,
    }
    report = audit(rep3_qodec)
    errors = [item for item in report.errors if "undeclared_angle" in item.summary]
    assert len(errors) == 1
    assert errors[0].where.startswith("layers[0].instruction_set")

    detached_errors = list(structural_issues(gadget))
    assert sum("undeclared_angle" in message for _, message in detached_errors) == 1


def test_uninspectable_action_does_not_hide_independent_reference_errors(
    tmp_path: Path,
) -> None:
    path = tmp_path / "conditional.isa.yaml"
    path.write_text(
        json.dumps(
            {
                "name": "conditional",
                "blocks": {},
                "instructions": [
                    {
                        "mnemonic": "measure",
                        "description": "draft",
                        "action": [{"observe": "Z_0", "if": ["enabled"]}],
                    }
                ],
            }
        )
    )
    logical = qc.InstructionSet.load(path)
    physical = qc.InstructionSet("physical")
    gadget = qc.Gadget(
        logical.instructions["measure"],
        qc.gadgets.Circuit(physical, "[]", format="yaml"),
        readouts=[["readouts[2]"]],
    )
    protocol = qc.Qodec([qc.Layer(logical, gadgets=[gadget]), qc.Layer(physical)])
    report = audit(protocol)
    assert len(report.errors) == 2
    assert any("action cannot be inspected" in item.summary for item in report.errors)
    assert any("readouts[2]" in item.summary for item in report.errors)


def test_unmatched_encoding_codes_are_checked_without_crossing_boundaries(
    rep3_qodec: qc.Qodec,
) -> None:
    gadget = rep3_qodec.layers[0].gadgets["measure_z"]
    code = qc.Code("unmatched", stabilizers=[], x=["X_0"], z=[])
    gadget.outputs = [qc.gadgets.Encoding(code, support=["0"])]
    issues = list(structural_issues(rep3_qodec))
    assert any(
        path == 'codes["unmatched"]' and "counts disagree" in message
        for path, message in issues
    )
    assert any("out: 1 encodings for 0 operands" in message for _, message in issues)
    assert not any("bound to different codes" in message for _, message in issues)
    report = audit(rep3_qodec)
    assert not any(item.rule == "code/invalid-algebra" for item in report.errors)
