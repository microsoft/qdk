# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Guards the secondary-source-span policy in both layers.

A node's own ``span`` covers the whole construct. Several nodes additionally
record where one part of themselves sits: a keyword, a bracketed index list, a
type as written. Those secondary spans are reachable so a caller can point at
the right piece of source, and they are excluded from structural comparison so
the same construct written at two different offsets still compares, hashes, and
renders the same.

That exclusion is easy to lose. The node macro derives each generated class's
``__eq__``, ``__hash__``, and ``__repr__`` field list from one accumulator, and a
secondary span stays out of it only because its field kind says so. The existing
equality tests compare two parses of one source, where offsets match, so they
cannot detect a span that wrongly joined equality. These tests can.
"""

from __future__ import annotations

from typing import Any

import pytest

from qasm_corpus import walk as _walk
from qdk.openqasm import parser, semantic
from qdk.openqasm.parser import Span

# One program exercising every construct that records a secondary span, and the
# same program shifted by a leading comment so every offset differs.
_SOURCE = """OPENQASM 3.0;
include "stdgates.inc";
#pragma qdk.box.open value here
qubit[3] q;
bit[3] c;
output bit outp;
float[64] fv = cos(0.5);
gate mygate(a) b { rx(a) b; }
def sub(int[32] x) -> int[32] { return x; }
int[32] r = sub(3);
def dyn(readonly array[int[32], #dim = 1] a) -> int[32] { return sizeof(a, 0); }
duration dd = durationof({x q[0];});
reset q[0];
c[0] = measure q[0];
gphase(0.5);
ctrl @ x q[0], q[1];
"""

_SHIFTED = "// a leading comment that moves every offset\n" + _SOURCE

# Every secondary span this API exposes, by the class that carries it. Written
# out rather than derived, because the macro's field kinds are a Rust-side
# distinction with no Python-side introspection: at runtime a span accessor is
# an ordinary getter and looks exactly like a scalar one.
_SECONDARY_SPANS = {
    "syntax": {
        "Annotation": ["value_span"],
        "DurationOf": ["name_span"],
        "IndexedIdentifier": ["index_span"],
        "Pragma": ["command_span", "value_span"],
        "QuantumGateModifier": ["modifier_keyword_span"],
        "QuantumMeasurement": ["measure_token_span"],
        "QuantumPhase": ["gphase_token_span"],
        "QuantumReset": ["reset_token_span"],
    },
    "semantic": {
        "Annotation": ["value_span"],
        "BuiltinFunctionCall": ["fn_name_span"],
        "ClassicalDeclaration": ["ty_span"],
        "DurationOf": ["fn_name_span"],
        "FunctionCall": ["fn_name_span"],
        "OutputDeclaration": ["ty_span"],
        "Pragma": ["command_span", "value_span"],
        "QuantumGate": ["gate_name_span"],
        "QuantumGateDefinition": ["name_span"],
        "QuantumGateModifier": ["modifier_keyword_span"],
        "QuantumMeasurement": ["measure_token_span"],
        "QuantumReset": ["reset_token_span"],
        "QubitArrayDeclaration": ["size_span"],
        "RuntimeSizeof": ["fn_name_span"],
        "SubroutineDefinition": ["return_type_span"],
    },
}


def _programs(source: str) -> dict[str, Any]:
    return {
        "syntax": parser.parse(source).program,
        "semantic": semantic.analyze(source).program,
    }


@pytest.mark.parametrize("layer", ["syntax", "semantic"])
def test_every_secondary_span_resolves_on_a_live_node(layer: str) -> None:
    """The inventory is real, not aspirational: each span appears in a parse."""
    program = _programs(_SOURCE)[layer]
    seen: set[tuple[str, str]] = set()
    for node in _walk(program):
        accessors = _SECONDARY_SPANS[layer].get(type(node).__name__)
        if not accessors:
            continue
        for accessor in accessors:
            value = getattr(node, accessor)
            assert value is None or isinstance(value, Span)
            if value is not None:
                assert value.lo <= value.hi
                seen.add((type(node).__name__, accessor))

    expected = {
        (class_name, accessor)
        for class_name, accessors in _SECONDARY_SPANS[layer].items()
        for accessor in accessors
    }
    # `Pragma.value_span` and `Annotation.value_span` are legitimately absent
    # when the construct carries no trailing value, so they are not required.
    optional = {("Pragma", "value_span"), ("Annotation", "value_span")}
    assert expected - seen <= optional, f"never observed: {sorted(expected - seen - optional)}"


@pytest.mark.parametrize("layer", ["syntax", "semantic"])
def test_shifting_the_source_moves_every_secondary_span(layer: str) -> None:
    """Without this, the equality test below could pass on identical spans."""
    original = _programs(_SOURCE)[layer]
    shifted = _programs(_SHIFTED)[layer]
    moved = 0
    for left, right in zip(_walk(original), _walk(shifted)):
        for accessor in _SECONDARY_SPANS[layer].get(type(left).__name__, ()):
            a, b = getattr(left, accessor), getattr(right, accessor)
            if a is not None and b is not None and (a.lo, a.hi) != (b.lo, b.hi):
                moved += 1
    assert moved >= 10, f"only {moved} secondary spans differed between the two parses"


@pytest.mark.parametrize("layer", ["syntax", "semantic"])
def test_no_secondary_span_affects_equality_hashing_or_repr(layer: str) -> None:
    """The same construct at two offsets stays equal, equally hashed, and equally rendered."""
    original = _programs(_SOURCE)[layer]
    shifted = _programs(_SHIFTED)[layer]
    for left, right in zip(_walk(original), _walk(shifted)):
        assert type(left) is type(right)
        assert left == right, f"{type(left).__name__} differs after a source shift"
        assert hash(left) == hash(right), f"{type(left).__name__} hashes differently"
        assert repr(left) == repr(right), f"{type(left).__name__} renders differently"


@pytest.mark.parametrize("layer", ["syntax", "semantic"])
def test_no_secondary_span_appears_in_children_or_repr(layer: str) -> None:
    """A span is a value, not a child, and never widens the rendered field list."""
    program = _programs(_SOURCE)[layer]
    for node in _walk(program):
        for accessor in _SECONDARY_SPANS[layer].get(type(node).__name__, ()):
            assert accessor not in repr(node), (
                f"{type(node).__name__}.{accessor} leaked into repr"
            )
        assert not any(isinstance(child, Span) for child in node.children())


def test_the_resolved_symbol_exposes_its_type_span_without_using_it() -> None:
    """The symbol view carries a type span and, like nodes, keeps it out of equality."""
    left = semantic.analyze(_SOURCE).symbols
    right = semantic.analyze(_SHIFTED).symbols
    pairs = list(zip(left.symbols(), right.symbols()))
    assert pairs, "the analysis produced no symbols"

    moved = 0
    for a, b in pairs:
        assert isinstance(a.ty_span, Span)
        if (a.ty_span.lo, a.ty_span.hi) != (b.ty_span.lo, b.ty_span.hi):
            moved += 1
        assert a == b, f"symbol {a.name} differs after a source shift"
        assert hash(a) == hash(b), f"symbol {a.name} hashes differently"
    assert moved, "no symbol type span moved, so the check proved nothing"
