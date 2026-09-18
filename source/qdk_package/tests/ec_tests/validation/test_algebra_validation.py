"""Algebra is checked by analysis, not by qodec persistence."""

import pytest
import qodec as qc

from qdk.ec import CodeProfile
from qdk.ec._analysis.propagation.isa_actions import build_clifford_images
from qdk.ec._audit.rules.code import CodeAlgebraRule
from qdk.ec._audit.rules.instruction_set import CliffordAlgebraRule
from qdk.ec._audit import Auditor, audit
from qdk.ec._audit import Diagnostic, Phase, Severity
from qdk.ec._audit.rules.qodec import StructuralValidationRule


def test_clifford_images_validate_implicit_identity_before_native_constructor() -> None:
    with pytest.raises(ValueError, match="must anticommute"):
        build_clifford_images({"X_0": "Z_0"}, {0: 0}, {0: 0}, 1)
    images = build_clifford_images({"X_0": "Z_0", "Z_0": "X_0"}, {0: 0}, {0: 0}, 1)
    assert [str(image) for image in images] == ["Z", "X"]


def test_clifford_images_reject_conflicting_aliases() -> None:
    with pytest.raises(ValueError, match="conflicting images"):
        build_clifford_images({"X_0": "X_0", "X_00": "Z_0"}, {0: 0}, {0: 0}, 1)


@pytest.mark.parametrize(
    ("stabilizers", "logical_x", "logical_z", "evidence"),
    [
        (["X_0", "Z_0"], [], [], "stabilizers[0] and stabilizers[1]"),
        ([], ["X_0"], ["X_0"], "x[0] and z[0]"),
        (["Z_0"], ["X_0"], ["Z_0"], "stabilizers[0] and x[0]"),
        ([], ["X_0", "Z_0"], ["Z_0", "X_0"], "x[0] and x[1]"),
    ],
)
def test_code_algebra_has_pair_evidence(
    stabilizers: list[str], logical_x: list[str], logical_z: list[str], evidence: str
) -> None:
    code = qc.Code("draft", stabilizers=stabilizers, x=logical_x, z=logical_z)
    diagnostics = list(CodeAlgebraRule()(code, qodec=qc.Qodec([])))
    assert diagnostics
    assert all(item.rule == "code/invalid-algebra" for item in diagnostics)
    assert any(evidence in item.summary for item in diagnostics)
    with pytest.raises(ValueError):
        CodeProfile(code)


def test_clifford_algebra_rule_checks_implicit_images() -> None:
    operand = qc.instructions.BlockOperand("qubit")
    instruction = qc.Instruction(
        "bad",
        inputs=[operand],
        outputs=[operand],
        action=[qc.actions.Clifford({"X_0": "Z_0"})],
    )
    instruction_set = qc.InstructionSet(
        "draft",
        blocks=[qc.instructions.Block("qubit", encodes=1)],
        instructions=[instruction],
    )
    diagnostics = list(CliffordAlgebraRule()(instruction_set, qodec=qc.Qodec([])))
    assert len(diagnostics) == 1
    assert diagnostics[0].rule == "instruction-set/invalid-clifford"
    assert "identity images" in diagnostics[0].detail


def test_invalid_code_blocks_dependent_gadgets(rep3_qodec: qc.Qodec) -> None:
    code = rep3_qodec.codes["repetition3"]
    code.z = list(code.x)
    report = audit(rep3_qodec)
    assert {item.rule for item in report.errors} == {"code/invalid-algebra"}
    assert not [
        item
        for item in report.diagnostics
        if item.rule.startswith("gadget/") and item.severity.name != "INFO"
    ]


def test_structural_findings_are_located_in_audit(rep3_qodec: qc.Qodec) -> None:
    gadget = rep3_qodec.layers[0].gadgets["idle"]
    gadget.checks = [["in[9].x[0]"]]
    report = audit(rep3_qodec)
    diagnostic = next(
        item for item in report.errors if item.rule == "qodec/invalid-structure"
    )
    assert diagnostic.where.startswith("layers[0].gadgets['idle']")
    assert "in[9].x[0]" in diagnostic.summary
    assert not [item for item in report.warnings if item.where == diagnostic.where]


def test_detached_gadget_uses_qodec_bounds_check(rep3_qodec: qc.Qodec) -> None:
    original = rep3_qodec.layers[0].gadgets["idle"]
    detached = qc.Gadget(
        original.implements,
        original.circuit,
        inputs=original.inputs,
        outputs=original.outputs,
        checks=[["in[9].x[0]"]],
    )
    report = Auditor().audit_gadget(detached, qodec=rep3_qodec)
    assert len(report.errors) == 1
    assert report.errors[0].rule == "qodec/invalid-structure"
    assert report.errors[0].where == "gadget['idle']"
    assert "in[9].x[0]" in report.errors[0].summary


def test_shared_invalid_instruction_set_blocks_all_occurrences(
    rep3_qodec: qc.Qodec,
) -> None:
    source, physical = rep3_qodec.layers
    source.instruction_set.blocks = []
    protocol = qc.Qodec([source, physical, source, physical])
    report = audit(protocol)
    assert any(item.rule == "qodec/invalid-structure" for item in report.errors)
    assert not [
        item
        for item in report.diagnostics
        if item.rule.startswith("gadget/") and item.severity is not Severity.INFO
    ]


def test_algebra_failure_does_not_block_independent_gadget(
    rep3_qodec: qc.Qodec,
) -> None:
    from collections.abc import Iterator

    class RecordAnalysis:
        name = "test/analyzed"
        severity = Severity.INFO
        phase = Phase.SEMANTIC
        target = qc.Gadget

        def __call__(self, target: object, *, qodec: qc.Qodec) -> Iterator[Diagnostic]:
            assert isinstance(target, qc.Gadget)
            yield Diagnostic(self.name, self.severity, target.implements.mnemonic, "")

    source, physical = rep3_qodec.layers
    source.instruction_set.blocks = [
        *source.instruction_set.blocks,
        qc.instructions.Block("unencoded", encodes=1),
    ]
    operand = qc.instructions.BlockOperand("unencoded")
    instruction = qc.Instruction("independent", inputs=[operand], outputs=[operand])
    source.instruction_set.instructions = [
        *source.instruction_set.instructions.values(),
        instruction,
    ]
    encoding = qc.gadgets.Encoding(
        qc.Code("qubit", stabilizers=[], x=["X_0"], z=["Z_0"]), support=["0"]
    )
    gadget = qc.Gadget(
        instruction,
        qc.gadgets.Circuit(physical.instruction_set, "[]", format="yaml"),
        inputs=[encoding],
        outputs=[encoding],
    )
    source.gadgets = {**source.gadgets, "independent": gadget}
    code = rep3_qodec.codes["repetition3"]
    code.z = list(code.x)
    report = Auditor(
        rules=[StructuralValidationRule(), CodeAlgebraRule(), RecordAnalysis()]
    ).audit(rep3_qodec)
    assert {item.rule for item in report.errors} == {"code/invalid-algebra"}, str(
        report
    )
    assert [item.summary for item in report.informational] == ["independent"]
