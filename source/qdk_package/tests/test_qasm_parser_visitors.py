# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Guards how the syntactic visitor dispatches and traverses.

A ``visit_<NodeType>`` callback must fire for exactly the nodes of that type,
including auxiliary nodes and recovery placeholders that are easy to leave out
of traversal. ``generic_visit`` must reach every child exactly once and in
source order, and the optional context argument must reach every callback that
accepts one without breaking callbacks that do not.
"""

from typing import Any, List, Tuple

import pytest

from qdk.openqasm import parser, semantic
from qdk.openqasm.parser import QASMVisitor, Statement


def test_error_statement_callback_dispatches() -> None:
    # A deliberately-invalid program (a trailing binary operator with no
    # right-hand operand) makes the parser emit an error statement node.
    result = parser.parse("OPENQASM 3.0; int a = 1 + ; ")
    assert result.has_errors

    error_nodes: List[Any] = []

    class Collector(QASMVisitor):
        def generic_visit(self, node: Any) -> None:
            if type(node).__name__ == "ErrorStatement":
                error_nodes.append(node)
            super().generic_visit(node)

    Collector().visit(result.program)
    assert error_nodes, "expected at least one ErrorStatement node in the tree"
    for node in error_nodes:
        assert isinstance(node, Statement)

    fired: List[Any] = []

    class ErrorVisitor(QASMVisitor):
        def visit_ErrorStatement(self, node: Any) -> None:
            fired.append(node)
            self.generic_visit(node)

    ErrorVisitor().visit(result.program)
    assert len(fired) >= 1
    for node in fired:
        assert type(node).__name__ == "ErrorStatement"
        assert isinstance(node, Statement)


def test_auxiliary_node_callbacks_dispatch() -> None:
    source = """OPENQASM 3.0;
    @tag value
    def f(int[8] a) -> int { return a; }
    int x;
    switch (x) { case 1 { x = 2; } }
    """
    fired: List[str] = []

    class AuxiliaryVisitor(QASMVisitor):
        def visit_Annotation(self, node: Any) -> None:
            fired.append("Annotation")

        def visit_SubroutineParameter(self, node: Any) -> None:
            fired.append("SubroutineParameter")
            self.generic_visit(node)

        def visit_SwitchCase(self, node: Any) -> None:
            fired.append("SwitchCase")
            self.generic_visit(node)

    AuxiliaryVisitor().visit(parser.parse(source).program)
    assert fired == ["Annotation", "SubroutineParameter", "SwitchCase"]


def test_syntax_type_expressions_are_visited_as_children() -> None:
    values: List[int] = []

    class TypeExpressionVisitor(QASMVisitor):
        def visit_IntegerLiteral(self, node: Any) -> None:
            values.append(node.value)
            self.generic_visit(node)

    program = parser.parse("OPENQASM 3.0; array[int[8], 2, 3] values;").program
    TypeExpressionVisitor().visit(program)

    assert values == [8, 2, 3]


def test_syntax_visitor_propagates_context_and_supports_legacy_callbacks() -> None:
    context = {"layer": "syntax"}
    seen: List[Tuple[str, Any]] = []

    class ContextVisitor(QASMVisitor):
        def visit_Annotation(self, node: Any, callback_context: Any = None) -> None:
            seen.append(("annotation", callback_context))

        def visit_ClassicalDeclaration(
            self, node: Any, callback_context: Any = None
        ) -> None:
            seen.append(("declaration", callback_context))
            self.generic_visit(node, callback_context)

        def visit_IntegerLiteral(
            self, node: Any, callback_context: Any = None
        ) -> None:
            seen.append((f"literal:{node.value}", callback_context))
            self.generic_visit(node, callback_context)

    program = parser.parse("OPENQASM 3.0;\n@tag\nint[8] value = 1;").program
    ContextVisitor().visit(program, context)

    # Children follow source order: the declared type precedes the initializer.
    assert [name for name, _ in seen] == [
        "declaration",
        "annotation",
        "literal:8",
        "literal:1",
    ]
    assert all(callback_context is context for _, callback_context in seen)

    legacy_names: List[str] = []

    class LegacyVisitor(QASMVisitor):
        def visit_Identifier(self, node: Any) -> None:
            legacy_names.append(node.name)

    LegacyVisitor().visit(program, context)
    assert legacy_names == ["value"]


def test_generic_traversal_reaches_each_annotation_exactly_once() -> None:
    source = """OPENQASM 3.0;
@one
@two 23
qubit q;
@three
int x = 1;
"""
    program = parser.parse(source).program

    identifiers: List[str] = []

    class AnnotationVisitor(QASMVisitor):
        def visit_Annotation(self, node: Any) -> None:
            identifiers.append(node.identifier)
            self.generic_visit(node)

    AnnotationVisitor().visit(program)

    # Each annotation is observed once, in source order, and annotations
    # precede the statement's own children.
    assert identifiers == ["one", "two", "three"]

    declaration = program.statements[-1]
    children = declaration.children()
    assert [type(child).__name__ for child in children[:1]] == ["Annotation"]
    assert children[0] == declaration.annotations[0]
    assert len(children) > 1


def test_a_typed_callback_fires_once_per_matching_node() -> None:
    class GateCounter(QASMVisitor):
        def __init__(self) -> None:
            self.count = 0

        def visit_QuantumGate(self, node: object) -> None:
            self.count += 1
            self.generic_visit(node)

    result = parser.parse("OPENQASM 3.0; qubit q; x q; y q; z q;")
    counter = GateCounter()
    counter.visit(result.program)
    assert counter.count == 3

def test_overriding_generic_visit_observes_every_node_in_the_tree() -> None:
    class NodeCounter(QASMVisitor):
        def __init__(self) -> None:
            self.count = 0

        def generic_visit(self, node: object) -> None:
            self.count += 1
            super().generic_visit(node)

    result = parser.parse("OPENQASM 3.0; qubit q; x q;")
    counter = NodeCounter()
    counter.visit(result.program)
    # Program + qubit decl + its identifier + gate + its name identifier + operand.
    assert counter.count > 3

def test_both_layers_export_the_same_visitor_class() -> None:
    assert parser.QASMVisitor is semantic.QASMVisitor


class _OpaqueCallback:
    """A callable that reports no signature, as some C built-ins do.

    Written as a double rather than borrowed from the standard library because
    no non-introspectable built-in both accepts one or two positional arguments
    and reports which it received.
    """

    def __init__(self) -> None:
        self.calls: List[Tuple[Any, ...]] = []

    def __call__(self, *args: Any) -> None:
        self.calls.append(args)

    @property
    def __signature__(self) -> Any:
        raise ValueError("no signature found")


def test_a_callback_with_no_introspectable_signature_follows_the_context() -> None:
    """The third arity outcome: decide from whether a context was supplied.

    Both introspectable outcomes are covered above. When a callback exposes no
    signature the visitor cannot learn its arity, and its documented fallback is
    to pass a context only when it holds one.
    """
    callback = _OpaqueCallback()

    class Visitor(QASMVisitor):
        pass

    visitor = Visitor()
    visitor.visit_Program = callback  # type: ignore[attr-defined]
    program = parser.parse("OPENQASM 3.0; qubit q;").program

    visitor.visit(program)
    visitor.visit(program, "context")

    assert callback.calls == [(program,), (program, "context")]
