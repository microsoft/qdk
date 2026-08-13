# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Proves the SyntaxNode and SemanticNode predicates answer correctly.

Most class names appear in both layers, so `Program` or `IntType` alone does not
say which tree produced a value. The two predicates answer that question, and
they are only worth exposing if they answer it exactly: no class may claim both
layers, and the four classes both trees share must claim neither.
"""

from __future__ import annotations

from qdk.openqasm import parser, semantic

# The classes both layers use, which belong to neither family. Change this only
# as a deliberate decision about the public predicate, never to make a test go
# green.
_SHARED = ("QASMNode", "Expression", "Statement", "Annotation")


def test_parsed_and_analyzed_programs_answer_correctly() -> None:
    source = "OPENQASM 3.0; qubit q; x q;"
    syntactic = parser.parse(source).program
    analyzed = semantic.analyze(source).program

    assert isinstance(syntactic, parser.SyntaxNode)
    assert not isinstance(syntactic, semantic.SemanticNode)
    assert isinstance(analyzed, semantic.SemanticNode)
    assert not isinstance(analyzed, parser.SyntaxNode)

    assert isinstance(syntactic.statements[0], parser.SyntaxNode)
    assert isinstance(analyzed.statements[0], semantic.SemanticNode)


def test_the_predicates_separate_colliding_class_names() -> None:
    """`IntType` and `Program` name a different class in each layer."""
    for name in ("Program", "IntType", "QuantumGate"):
        syntactic = getattr(parser, name)
        resolved = getattr(semantic, name)
        assert syntactic is not resolved, f"{name} is the same class in both layers"
        assert issubclass(syntactic, parser.SyntaxNode)
        assert not issubclass(syntactic, semantic.SemanticNode)
        assert issubclass(resolved, semantic.SemanticNode)
        assert not issubclass(resolved, parser.SyntaxNode)


def test_a_resolved_type_is_a_semantic_node() -> None:
    """Resolved types are not QASMNodes, so they must join the family explicitly."""
    program = semantic.analyze("OPENQASM 3.0; int[8] x = 1;").program
    resolved = program.statements[0].type

    assert isinstance(resolved, semantic.Type)
    assert isinstance(resolved, semantic.SemanticNode)
    assert not isinstance(resolved, parser.SyntaxNode)


def test_the_shared_classes_belong_to_neither_family() -> None:
    """Documented behavior: asking a shared class which tree it came from has no answer."""
    for name in _SHARED:
        cls = getattr(parser, name)
        assert cls is getattr(semantic, name), (
            f"{name} is documented as shared but the two modules export "
            "different classes"
        )
        assert not issubclass(cls, parser.SyntaxNode), (
            f"{name} is used by both trees, so calling it a SyntaxNode is false"
        )
        assert not issubclass(cls, semantic.SemanticNode), (
            f"{name} is used by both trees, so calling it a SemanticNode is false"
        )


def test_no_class_belongs_to_both_families() -> None:
    """A class answering True to both questions would make the predicates useless."""
    both = sorted(
        name
        for module in (parser, semantic)
        for name in module.__all__
        for cls in [getattr(module, name)]
        if isinstance(cls, type)
        and issubclass(cls, parser.SyntaxNode)
        and issubclass(cls, semantic.SemanticNode)
    )
    assert not both, "these classes claim both layers:\n" + "\n".join(both)


def test_results_and_symbol_tables_join_neither_family() -> None:
    """The predicates answer about tree membership, not about every export."""
    for value in (
        parser.parse("OPENQASM 3.0;"),
        semantic.analyze("OPENQASM 3.0;"),
        semantic.analyze("OPENQASM 3.0; qubit q;").symbols,
    ):
        assert not isinstance(value, parser.SyntaxNode)
        assert not isinstance(value, semantic.SemanticNode)
