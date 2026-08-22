"""Unit tests for the qodec property-path atom vocabulary.

:mod:`qdk.ec._references` is the single source of truth for the property-path
DSL; every other module delegates to it and matches on atom types. The cases
below pin the reference shapes it must accept and the text it must render back.
"""

from __future__ import annotations

from qdk.ec._references import (
    LogicalSign,
    Outcome,
    StabilizerSign,
    logical_signs_of,
    outcome_equation,
    outcomes_of,
    parse_equation,
    parse_equations,
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


def test_parse_equation_drops_unmodelled_shapes() -> None:
    assert parse_equation(["checks[2]", "readouts[1]", "in.block.stabilizers[1]"]) == ()


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
    assert logical_signs_of(equation, side="in") == [LogicalSign("in", 0, "z", 1)]
    assert logical_signs_of(equation, side="out") == []


def test_sign_keys_are_side_independent() -> None:
    assert StabilizerSign("in", 0, 2).key == StabilizerSign("out", 0, 2).key
    assert LogicalSign("in", 1, "x", 0).key == LogicalSign("out", 1, "x", 0).key


def test_outcome_equation_builds_a_record_xor() -> None:
    assert outcome_equation([2, 5]) == (Outcome(2), Outcome(5))
