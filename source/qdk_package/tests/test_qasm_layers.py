# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Proves the SyntaxNode and SemanticNode predicates partition the two trees.

The predicates are only worth exposing if they are exhaustive. A predicate that
silently misses a node kind is worse than the ``__module__`` convention it
replaces, because it answers confidently and wrongly. These tests fail when a
node class is added without joining a family, and when a class silently enters
or leaves the documented set that belongs to neither.
"""

from __future__ import annotations

from typing import Dict

from qdk.openqasm import parser, semantic

# The classes both layers use, which belong to neither family. Change this only
# as a deliberate decision about the public predicate, never to make a test go
# green.
_SHARED = ("QASMNode", "Expression", "Statement", "Annotation")

# The tree roots of each layer. `Type` is a second semantic root because
# resolved types are not `QASMNode`s yet are unmistakably semantic.
_SYNTAX_ROOTS = (parser.QASMNode,)
_SEMANTIC_ROOTS = (parser.QASMNode, semantic.Type)


def _exported_classes(module: object) -> Dict[str, type]:
    return {
        name: getattr(module, name)
        for name in getattr(module, "__all__")
        if isinstance(getattr(module, name, None), type)
    }


def _tree_classes(module: object, roots: tuple) -> Dict[str, type]:
    return {
        name: cls
        for name, cls in _exported_classes(module).items()
        if issubclass(cls, roots)
    }


def test_every_syntactic_tree_class_is_a_syntax_node() -> None:
    unregistered = sorted(
        name
        for name, cls in _tree_classes(parser, _SYNTAX_ROOTS).items()
        if name not in _SHARED and not issubclass(cls, parser.SyntaxNode)
    )
    assert not unregistered, (
        "these syntactic classes are not SyntaxNodes, so the predicate answers "
        "False for real syntax nodes:\n" + "\n".join(unregistered)
    )


def test_every_semantic_tree_class_is_a_semantic_node() -> None:
    unregistered = sorted(
        name
        for name, cls in _tree_classes(semantic, _SEMANTIC_ROOTS).items()
        if name not in _SHARED and not issubclass(cls, semantic.SemanticNode)
    )
    assert not unregistered, (
        "these semantic classes are not SemanticNodes, so the predicate answers "
        "False for real semantic nodes:\n" + "\n".join(unregistered)
    )


def test_no_class_belongs_to_both_families() -> None:
    """A class answering True to both questions would make the predicate useless."""
    both = sorted(
        name
        for module in (parser, semantic)
        for name, cls in _exported_classes(module).items()
        if issubclass(cls, parser.SyntaxNode) and issubclass(cls, semantic.SemanticNode)
    )
    assert not both, "these classes claim both layers:\n" + "\n".join(both)


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


def test_the_shared_set_is_exactly_the_classes_both_trees_use() -> None:
    """Fails if a class silently joins or leaves the exception set."""
    syntax = _tree_classes(parser, _SYNTAX_ROOTS)
    semantics = _tree_classes(semantic, _SEMANTIC_ROOTS)
    actually_shared = sorted(
        name
        for name in syntax.keys() & semantics.keys()
        if syntax[name] is semantics[name]
    )
    assert actually_shared == sorted(_SHARED), (
        "the set of classes both trees use has changed. Update _SHARED, the "
        "documentation in qdk/openqasm/_layers.py, and the module docstrings "
        "together, as a decision about the predicate's contract."
    )


def test_non_tree_exports_join_neither_family() -> None:
    """Results, symbol tables, and shared enums are not tree members."""
    families = (parser.SyntaxNode, semantic.SemanticNode)
    for module, roots in ((parser, _SYNTAX_ROOTS), (semantic, _SEMANTIC_ROOTS)):
        tree = _tree_classes(module, roots)
        misclassified = sorted(
            name
            for name, cls in _exported_classes(module).items()
            if name not in tree
            and cls not in families
            and (
                issubclass(cls, parser.SyntaxNode)
                or issubclass(cls, semantic.SemanticNode)
            )
        )
        assert not misclassified, (
            f"these {module.__name__} exports are not tree classes but were "
            "registered anyway:\n" + "\n".join(misclassified)
        )


def test_the_partition_actually_covers_the_surface() -> None:
    """Otherwise a registration bug that empties a family would pass silently."""
    syntax = _tree_classes(parser, _SYNTAX_ROOTS)
    semantics = _tree_classes(semantic, _SEMANTIC_ROOTS)
    assert len(syntax) > 60, len(syntax)
    assert len(semantics) > 60, len(semantics)


def test_the_predicate_separates_colliding_type_names() -> None:
    """The type vocabulary is where the two layers collide hardest."""
    colliding = sorted(
        name
        for name in _tree_classes(parser, _SYNTAX_ROOTS).keys()
        & _tree_classes(semantic, _SEMANTIC_ROOTS).keys()
        if name.endswith("Type")
    )
    assert colliding, "expected the two layers to share type class names"
    for name in colliding:
        assert issubclass(getattr(parser, name), parser.SyntaxNode), name
        assert issubclass(getattr(semantic, name), semantic.SemanticNode), name


def test_parsed_and_analyzed_programs_answer_correctly() -> None:
    """The class-level checks above are only useful if instances agree."""
    source = "OPENQASM 3.0; qubit q; x q;"
    syntactic = parser.parse(source).program
    analyzed = semantic.analyze(source).program

    assert isinstance(syntactic, parser.SyntaxNode)
    assert not isinstance(syntactic, semantic.SemanticNode)
    assert isinstance(analyzed, semantic.SemanticNode)
    assert not isinstance(analyzed, parser.SyntaxNode)

    assert isinstance(analyzed.statements[0], semantic.SemanticNode)
    assert isinstance(syntactic.statements[0], parser.SyntaxNode)
