"""Unit tests for the canonical qodec property-path atom parsers.

These helpers in :mod:`qdk.ec._qodec_compat` are the single source of
truth for the v3.4 property-path atom DSL; every other module delegates
to them. The cases below pin the dot/bracket/selector shapes those
parsers must accept.
"""
from __future__ import annotations

import pytest

from qdk.ec._qodec_compat import (
    EncodingAtom,
    outcome_index_of_atom,
    outcome_indices,
    parse_encoding_atom,
    parse_stabilizer_atom,
)


def test_outcome_indices_accepts_dot_and_bracket_shapes() -> None:
    assert outcome_indices(["body.readouts.0", "body.readouts[3]"]) == [0, 3]


def test_outcome_indices_expands_bracket_selectors() -> None:
    assert outcome_indices(["body.readouts[1:4]"]) == [1, 2, 3]
    assert outcome_indices(["body.readouts[0,2,5]"]) == [0, 2, 5]


def test_outcome_indices_ignores_unrelated_atoms() -> None:
    assert not outcome_indices(["in[0].stabilizers[0]", "readouts[1]"])


def test_outcome_index_of_atom_shapes() -> None:
    assert outcome_index_of_atom("body.readouts.2") == 2
    assert outcome_index_of_atom("body.readouts[4]") == 4
    assert outcome_index_of_atom("7") == 7


def test_outcome_index_of_atom_rejects_multi_index_selector() -> None:
    with pytest.raises(ValueError):
        outcome_index_of_atom("body.readouts[0:2]")


def test_parse_encoding_atom_dot_and_bracket() -> None:
    assert parse_encoding_atom("in[0].stabilizers[1]") == EncodingAtom(
        side="in", entry=0, basis="stabilizers", index=1
    )
    assert parse_encoding_atom("out[2].z.3") == EncodingAtom(
        side="out", entry=2, basis="z", index=3
    )


def test_parse_encoding_atom_rejects_other_shapes() -> None:
    assert parse_encoding_atom("body.readouts[0]") is None
    assert parse_encoding_atom("checks[2]") is None
    # The removed named-operand form is rejected.
    assert parse_encoding_atom("in.block.stabilizers[1]") is None


def test_parse_stabilizer_atom_side_filtering() -> None:
    assert parse_stabilizer_atom("in[0].stabilizers[2]") == (0, 2)
    assert parse_stabilizer_atom("in[0].stabilizers[2]", side="in") == (0, 2)
    assert parse_stabilizer_atom("in[0].stabilizers[2]", side="out") is None


def test_parse_stabilizer_atom_rejects_non_stabilizer_basis() -> None:
    assert parse_stabilizer_atom("out[1].x[0]") is None
