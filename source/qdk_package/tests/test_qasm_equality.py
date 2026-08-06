# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Guards that equality and hashing are structural, paired, and consistent.

Nodes used to compare by identity, so two parses of the same source were never
equal. These tests pin the replacement contract: equality compares the concrete
type and the participating attributes, source positions never participate, and
every class that defines ``__eq__`` also defines ``__hash__``.

The pairing check is an introspection sweep rather than a corpus test on
purpose. PyO3 leaves ``tp_hash`` inherited when a class defines only ``__eq__``,
so a missing ``__hash__`` produces a silently inconsistent class that comparing
sample nodes would not reveal.
"""

from __future__ import annotations

from typing import Any, Iterator, List

import pytest

from qdk.openqasm import parser, semantic

# Bases exist only for `isinstance` dispatch and are never instantiated.
_ABSTRACT = {
    "QASMNode",
    "Expression",
    "Statement",
    "ClassicalType",
    "SemanticExpression",
    "SemanticStatement",
}

_SOURCE = """OPENQASM 3.1;
include "stdgates.inc";
@my.annotation payload
qubit[2] q;
bit[2] c;
int[8] i = 0;
const angle a = pi/2;
const duration d = 100ns;
array[int[8], 2] arr = {1, 2};
h q[0];
ctrl @ x q[0], q[1];
c[0] = measure q[0];
h $0;
for int[8] k in [0:3] {
  i += 1;
}
for int[8] m in {1, 2} {
  i += 1;
}
switch (i) {
  case 0 { x q[0]; }
  default { y q[0]; }
}
def f(int[8] n) -> int[8] {
  return n;
}
"""


def _classes() -> Iterator[tuple[str, type]]:
    roots = (parser.QASMNode, semantic.Type)
    seen: set[int] = set()
    for module in (parser, semantic):
        for name in module.__all__:
            value = getattr(module, name, None)
            if (
                isinstance(value, type)
                and issubclass(value, roots)
                and name not in _ABSTRACT
                and id(value) not in seen
            ):
                seen.add(id(value))
                yield f"{module.__name__}.{name}", value


def _walk(node: Any) -> Iterator[Any]:
    yield node
    for child in node.children():
        yield from _walk(child)


def _nodes(source: str = _SOURCE) -> List[Any]:
    program = semantic.analyze(source).program
    assert program is not None
    return list(_walk(program))


def test_every_concrete_class_defines_its_own_equality() -> None:
    missing = [name for name, cls in _classes() if "__eq__" not in vars(cls)]
    assert not missing, "classes left on identity equality:\n" + "\n".join(missing)


def test_every_class_defining_equality_also_defines_hash() -> None:
    """Catches PyO3's failure mode: `tp_hash` stays inherited and identity-based.

    A class in this state is silently inconsistent rather than unhashable, so
    only an introspection sweep finds it.
    """
    unpaired = [
        name
        for name, cls in _classes()
        if "__eq__" in vars(cls) and "__hash__" not in vars(cls)
    ]
    assert not unpaired, "classes with __eq__ but no __hash__:\n" + "\n".join(unpaired)


def test_no_class_was_left_unhashable() -> None:
    """Catches Python's opposite failure mode: `__hash__` set to None.

    Defining `__eq__` in a Python class body sets `__hash__` to None unless it is
    redefined, which is why this needs its own check.
    """
    unhashable = [name for name, cls in _classes() if cls.__hash__ is None]
    assert not unhashable, "unhashable classes:\n" + "\n".join(unhashable)


def test_two_analyses_of_one_source_are_equal_and_hash_equally() -> None:
    first = semantic.analyze(_SOURCE).program
    second = semantic.analyze(_SOURCE).program
    assert first == second
    assert hash(first) == hash(second)


def test_two_parses_of_one_source_are_equal_and_hash_equally() -> None:
    first = parser.parse(_SOURCE).program
    second = parser.parse(_SOURCE).program
    assert first == second
    assert hash(first) == hash(second)


def test_equal_nodes_hash_equally_across_the_whole_tree() -> None:
    """The consistency half of the contract, checked pairwise over a corpus."""
    first = _nodes()
    second = _nodes()
    assert len(first) > 60, f"corpus only produced {len(first)} nodes"
    for left, right in zip(first, second):
        assert left == right, f"{left!r} != {right!r}"
        assert hash(left) == hash(right), f"equal nodes hash differently: {left!r}"


def test_source_position_does_not_participate() -> None:
    without_padding = semantic.analyze("OPENQASM 3.0;\nint[8] v = 1;\n").program
    with_padding = semantic.analyze("OPENQASM 3.0;\n\n\n\nint[8] v = 1;\n").program
    left = without_padding.statements[-1]
    right = with_padding.statements[-1]

    assert left.span != right.span
    assert left == right
    assert hash(left) == hash(right)


def test_an_annotation_ignores_its_value_span() -> None:
    """`value_span` is a Span-typed field that is not the inherited `span`."""
    left = semantic.analyze("OPENQASM 3.0;\n@tag body\nqubit q;\n").program
    right = semantic.analyze("OPENQASM 3.0;\n\n@tag body\nqubit q;\n").program
    first = left.statements[-1].annotations[0]
    second = right.statements[-1].annotations[0]

    assert first.value_span != second.value_span
    assert first == second
    assert hash(first) == hash(second)


def test_different_concrete_types_never_compare_equal() -> None:
    """Two empty-bodied classes would otherwise be indistinguishable."""
    broke = semantic.analyze("OPENQASM 3.1;\nfor int[8] i in [0:2] {\n break;\n}\n")
    kept = semantic.analyze("OPENQASM 3.1;\nfor int[8] i in [0:2] {\n continue;\n}\n")

    def only(program: Any, name: str) -> Any:
        found = [n for n in _walk(program) if type(n).__name__ == name]
        assert len(found) == 1
        return found[0]

    break_statement = only(broke.program, "BreakStatement")
    continue_statement = only(kept.program, "ContinueStatement")
    assert break_statement != continue_statement
    assert len({break_statement, continue_statement}) == 2


def test_structurally_different_programs_are_not_equal() -> None:
    left = semantic.analyze("OPENQASM 3.0;\nint[8] v = 1;\n").program
    right = semantic.analyze("OPENQASM 3.0;\nint[8] v = 2;\n").program
    assert left != right


def test_a_node_is_not_equal_to_an_unrelated_object() -> None:
    node = semantic.analyze("OPENQASM 3.0;\nint[8] v = 1;\n").program
    assert node != "not a node"
    assert node is not None


def test_hashing_a_program_succeeds_despite_the_source_document() -> None:
    """`SourceDocument` is `eq` without `hash`, so `document` must not participate."""
    program = semantic.analyze(_SOURCE).program
    assert hash(program) is not None
    assert hash(parser.parse(_SOURCE).program) is not None


def test_nodes_work_as_set_and_dict_members() -> None:
    first = semantic.analyze(_SOURCE).program
    second = semantic.analyze(_SOURCE).program
    assert len({first, second}) == 1
    assert {first: "value"}[second] == "value"


@pytest.mark.parametrize(
    ("left_source", "right_source", "expected_equal"),
    [
        ("const angle a = pi/2;", "const angle a = pi/2;", True),
        ("const angle a = pi/2;", "const angle a = pi/4;", False),
        ("const duration d = 100ns;", "const duration d = 100ns;", True),
        ("const duration d = 100ns;", "const duration d = 100us;", False),
    ],
)
def test_value_classes_compare_by_value(
    left_source: str, right_source: str, expected_equal: bool
) -> None:
    def constant(declaration: str) -> Any:
        program = semantic.analyze(f"OPENQASM 3.0;\n{declaration}\n").program
        return program.statements[-1].init_expr.const_value

    left = constant(left_source)
    right = constant(right_source)
    assert (left == right) is expected_equal
    if expected_equal:
        assert hash(left) == hash(right)


def test_type_nodes_compare_by_value() -> None:
    assert semantic.analyze("OPENQASM 3.0;\nint[8] v;\n").program.statements[
        -1
    ].type == semantic.analyze("OPENQASM 3.0;\nint[8] w;\n").program.statements[-1].type
    assert (
        semantic.analyze("OPENQASM 3.0;\nint[8] v;\n").program.statements[-1].type
        != semantic.analyze("OPENQASM 3.0;\nint[16] v;\n").program.statements[-1].type
    )


def test_the_pairing_sweep_actually_covers_the_surface() -> None:
    """A guard that would otherwise pass vacuously if enumeration broke."""
    classes = list(_classes())
    assert len(classes) > 140, f"only found {len(classes)} concrete classes"


def test_symbol_table_position_does_not_participate() -> None:
    """A reference to `v` is the same node wherever `v` sits in the table.

    The comparison is between *subtrees* rather than whole programs, because
    two programs whose symbol tables differ must have differing declarations,
    which makes the programs structurally different for an unrelated reason.
    """
    early = semantic.analyze("OPENQASM 3.0;\nint[8] v = 1;\nv;\n").program
    late = semantic.analyze(
        "OPENQASM 3.0;\nint[8] a = 0;\nint[8] b = 0;\nint[8] v = 1;\nv;\n"
    ).program

    left = early.statements[-1]
    right = late.statements[-1]

    # The declarations really did land at different table positions, so the
    # assertion below is not vacuous.
    left_symbol = [n for n in _walk(left) if type(n).__name__ == "Identifier"][0]
    right_symbol = [n for n in _walk(right) if type(n).__name__ == "Identifier"][0]
    assert left_symbol.symbol.id != right_symbol.symbol.id

    assert left == right
    assert hash(left) == hash(right)

