"""Unit tests for the qodec property-path atom vocabulary.

qodec parses paths; :mod:`qdk.ec._references` interprets their parity roles.
These cases pin the accepted shapes and preserve every parsed term.
"""

from __future__ import annotations

import pytest
from qodec import Reference

from qdk.ec._references import (
    Basis,
    LogicalSign,
    Outcome,
    ReadoutSign,
    Side,
    StabilizerSign,
    outcome_equation,
    outcomes_of,
    parse_equation,
    parse_equations,
    reference_term,
    reference_terms,
    stabilizer_signs_of,
)


def test_parse_equation_reads_each_atom_shape() -> None:
    assert parse_equation(
        ["circuit.readouts[0]", "in[1].stabilizers[2]", "out[3].z[4]"]
    ) == (
        Outcome(0),
        StabilizerSign("in", 1, 2),
        LogicalSign("out", 3, "z", 4),
    )


def test_reference_interpretation_is_separate_from_general_addresses() -> None:
    assert tuple(reference_terms(Reference("readouts[2,0,2]"))) == (
        ReadoutSign(2),
        ReadoutSign(0),
        ReadoutSign(2),
    )
    assert tuple(reference_terms("in[0].z[00:01]")) == (LogicalSign("in", 0, "z", 0),)
    assert tuple(reference_terms("circuit.readouts[00:01]")) == (Outcome(0),)
    for path in [
        'metadata["readouts"][0]',
        'layers[0].gadgets["M"].in[0].z[0]',
        "in[0:1].z[0]",
        "in[0].code.z[0]",
    ]:
        assert Reference(path).segments
        with pytest.raises(ValueError, match="not a parity reference"):
            reference_terms(path)


def test_large_scalar_targets_stop_after_two_terms(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    import qodec as qc
    from qdk.ec._frames import FrameMap
    from qdk.ec._analysis.propagation.interpreter import _condition_indices

    created = []
    logical_init = LogicalSign.__init__
    outcome_init = Outcome.__init__

    def logical(
        self: LogicalSign, side: Side, entry: int, basis: Basis, index: int
    ) -> None:
        created.append(index)
        assert len(created) <= 2
        logical_init(self, side, entry, basis, index)

    def outcome(self: Outcome, index: int) -> None:
        created.append(index)
        assert len(created) <= 2
        outcome_init(self, index)

    monkeypatch.setattr(LogicalSign, "__init__", logical)
    monkeypatch.setattr(Outcome, "__init__", outcome)
    gadget = qc.Gadget(
        qc.Instruction("draft"),
        qc.gadgets.Circuit(qc.InstructionSet("draft"), "opaque"),
        frames={"out[0].z[0:1048576]": []},
    )
    with pytest.raises(ValueError, match="exactly one"):
        FrameMap(gadget)
    assert created == [0, 1]
    created.clear()
    with pytest.raises(ValueError, match="exactly one"):
        _condition_indices(
            qc.actions.Condition(["bit"]), {"bit": "circuit.readouts[0:1048576]"}, 1
        )
    assert created == [0, 1]
    created.clear()
    terms = reference_terms("out[0].z[0:1048576]")
    assert created == []
    assert next(terms) == LogicalSign("out", 0, "z", 0)


def test_singleton_terms_accept_index_and_singleton_slices() -> None:
    assert reference_term("out[0].z[0:1]") == LogicalSign("out", 0, "z", 0)
    assert reference_term("circuit.readouts[0]") == Outcome(0)


@pytest.mark.parametrize(
    "path",
    ["circuit.readouts[0]", "circuit.readouts[00:01]", "circuit.readouts[0:2:2]"],
)
def test_argument_audit_and_execution_accept_singleton_readout_selectors(
    path: str,
) -> None:
    import qodec as qc
    from qdk.ec._audit._structure import _argument_issue
    from qdk.ec._analysis.propagation.interpreter import _condition_indices

    gadget = qc.Gadget(
        qc.Instruction("draft"),
        qc.gadgets.Circuit(qc.InstructionSet("draft"), "opaque"),
    )
    bit = qc.instructions.Parameter("bit", "bit")
    text = qc.instructions.Parameter("text", "string")
    condition = qc.actions.Condition(["bit"])
    assert _argument_issue(path, bit, gadget, 1) is None
    assert _condition_indices(condition, {"bit": path}, 1) == ([0], True)
    assert (
        _argument_issue(path, text, gadget, 1)
        == "a circuit readout can bind only a bit parameter"
    )
    assert "out of bounds" in str(_argument_issue(path, bit, gadget, 0))
    with pytest.raises(ValueError, match="preceding circuit readout"):
        _condition_indices(condition, {"bit": path}, 0)
    assert _argument_issue("ordinary text", text, gadget, 1) is None
    for invalid in [
        "circuit.readouts[0:2]",
        "circuit.readouts[0,0]",
        "circuit.readouts[0:0]",
        'metadata["bit"]',
    ]:
        assert _argument_issue(invalid, bit, gadget, 2) is not None
        with pytest.raises(ValueError):
            _condition_indices(condition, {"bit": invalid}, 2)


def test_parse_equation_expands_bracket_selectors() -> None:
    assert parse_equation(["circuit.readouts[1:4]"]) == (
        Outcome(1),
        Outcome(2),
        Outcome(3),
    )
    assert parse_equation(["circuit.readouts[0,2,5]"]) == (
        Outcome(0),
        Outcome(2),
        Outcome(5),
    )


def test_parse_equation_preserves_declared_readout_terms() -> None:
    equation = parse_equation(["readouts[1,0,1]", "circuit.readouts[2]", 1])
    assert equation == (ReadoutSign(1), ReadoutSign(0), ReadoutSign(1), Outcome(2), 1)
    assert outcomes_of(equation) == [2]


def test_readout_equation_preserves_dependencies() -> None:
    import qodec as qc
    from qdk.ec._readouts import readout_equation

    gadget = qc.Gadget(
        qc.Instruction("draft", flags=["reject"]),
        qc.gadgets.Circuit(qc.InstructionSet("draft"), "opaque", format="unknown"),
        readouts=[["readouts[1]", "circuit.readouts[2]", 1]],
    )
    assert readout_equation(gadget.readouts[0]) == (ReadoutSign(1), Outcome(2), 1)


@pytest.mark.parametrize(
    "text", ["checks[2]", "in.block.stabilizers[1]", "metadata", ""]
)
def test_parse_equation_rejects_invalid_paths(text: str) -> None:
    with pytest.raises(ValueError):
        parse_equation([text])


def test_parse_equation_expands_typed_encoding_references() -> None:
    assert parse_equation(
        [Reference("in[0].stabilizers[0:2]"), Reference("out[1].x[1,3]")]
    ) == (
        StabilizerSign("in", 0, 0),
        StabilizerSign("in", 0, 1),
        LogicalSign("out", 1, "x", 1),
        LogicalSign("out", 1, "x", 3),
    )


def test_parse_equations_parses_a_whole_check_list() -> None:
    assert parse_equations([["circuit.readouts[0]"], ["out[0].stabilizers[1]"]]) == (
        (Outcome(0),),
        (StabilizerSign("out", 0, 1),),
    )


def test_atoms_render_back_to_their_reference_text() -> None:
    for text in (
        "circuit.readouts[7]",
        "in[0].stabilizers[2]",
        "out[1].x[3]",
    ):
        (atom,) = parse_equation([text])
        assert str(atom) == text


def test_outcomes_of_selects_only_measurement_records() -> None:
    equation = parse_equation(
        ["circuit.readouts[0]", "in[0].stabilizers[0]", "circuit.readouts[3]"]
    )
    assert outcomes_of(equation) == [0, 3]


def test_sign_selectors_filter_by_side() -> None:
    equation = parse_equation(
        ["in[0].stabilizers[2]", "out[1].stabilizers[0]", "in[0].z[1]"]
    )
    assert stabilizer_signs_of(equation, side="in") == [StabilizerSign("in", 0, 2)]
    assert stabilizer_signs_of(equation, side="out") == [StabilizerSign("out", 1, 0)]
    assert len(stabilizer_signs_of(equation)) == 2


def test_sign_keys_are_side_independent() -> None:
    assert StabilizerSign("in", 0, 2).key == StabilizerSign("out", 0, 2).key
    assert LogicalSign("in", 1, "x", 0).key == LogicalSign("out", 1, "x", 0).key


def test_outcome_equation_builds_a_record_xor() -> None:
    assert outcome_equation([2, 5]) == (Outcome(2), Outcome(5))
