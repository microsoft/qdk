# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Guards how the semantic visitor dispatches and traverses.

The analyzed tree differs from the parsed one in ways a visitor must handle:
broadcast gate calls expand into one node per qubit, and declared widths move
onto the resolved type instead of remaining child expressions. These tests pin
typed dispatch, context propagation, and single-visit traversal over that tree.
"""

from typing import Any, List, Tuple

from qdk.openqasm import semantic
from qdk.openqasm.semantic import QASMVisitor, Statement


def test_broadcast_semantic_visitor_observes_each_projected_gate() -> None:
    program = semantic.analyze(
        'OPENQASM 3.0; include "stdgates.inc"; qubit[4] q; h q;'
    ).program
    visited: List[Any] = []

    class GateVisitor(QASMVisitor):
        def visit_QuantumGate(self, node: Any) -> None:
            visited.append(node)
            self.generic_visit(node)

    GateVisitor().visit(program)

    assert len(visited) == 4
    assert [gate.qubits[0].indices[0].value for gate in visited] == [0, 1, 2, 3]


def test_error_statement_callback_dispatches() -> None:
    # A deliberately-invalid program (a trailing binary operator with no
    # right-hand operand) makes the analyzer emit an error statement node.
    result = semantic.analyze("OPENQASM 3.0; int a = 1 + ; ")
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


def test_semantic_visitor_propagates_context_and_supports_legacy_callbacks() -> None:
    context = {"layer": "semantic"}
    seen: List[Tuple[str, Any]] = []

    class ContextVisitor(QASMVisitor):
        def visit_Annotation(self, node: Any, callback_context: Any = None) -> None:
            seen.append(("annotation", callback_context))

        def visit_ClassicalDeclaration(
            self, node: Any, callback_context: Any = None
        ) -> None:
            seen.append(("declaration", callback_context))
            self.generic_visit(node, callback_context)

        def visit_LiteralExpression(
            self, node: Any, callback_context: Any = None
        ) -> None:
            seen.append((f"literal:{node.value}", callback_context))
            self.generic_visit(node, callback_context)

    program = semantic.analyze("OPENQASM 3.0;\n@tag\nint[8] value = 1; value;").program
    ContextVisitor().visit(program, context)

    # The declared width is on the resolved type rather than a child expression,
    # so traversal no longer reports the `8` as a literal.
    assert [name for name, _ in seen] == [
        "declaration",
        "annotation",
        "literal:1",
    ]
    assert all(callback_context is context for _, callback_context in seen)

    legacy_names: List[str] = []

    class LegacyVisitor(QASMVisitor):
        def visit_Identifier(self, node: Any) -> None:
            if node.name is not None:
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
    program = semantic.analyze(source).program

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


def test_auxiliary_node_callbacks_dispatch() -> None:
    source = """OPENQASM 3.1;
    include "stdgates.inc";
    @tag value
    def f(int[8] a) -> int { return a; }
    bit[4] bits;
    let slice = bits[1:2];
    int selector;
    switch (selector) { case 1 { selector = 2; } }
    qubit[2] q;
    ctrl @ x q[0], q[1];
    """
    fired: List[str] = []

    class AuxiliaryVisitor(QASMVisitor):
        def visit_Annotation(self, node: Any) -> None:
            fired.append("Annotation")

        def visit_SubroutineParameter(self, node: Any) -> None:
            fired.append("SubroutineParameter")
            self.generic_visit(node)

        def visit_RangeDefinition(self, node: Any) -> None:
            fired.append("RangeDefinition")
            self.generic_visit(node)

        def visit_SwitchCase(self, node: Any) -> None:
            fired.append("SwitchCase")
            self.generic_visit(node)

        def visit_QuantumGateModifier(self, node: Any) -> None:
            fired.append("QuantumGateModifier")
            self.generic_visit(node)

    result = semantic.analyze(source)
    assert not result.has_errors
    AuxiliaryVisitor().visit(result.program)
    assert fired == [
        "Annotation",
        "SubroutineParameter",
        "RangeDefinition",
        "SwitchCase",
        "QuantumGateModifier",
    ]
