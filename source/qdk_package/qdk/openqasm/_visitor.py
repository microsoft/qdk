# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""A read-only visitor for the OpenQASM AST node hierarchies.

:class:`QASMVisitor` is a single base class that walks either the syntactic
tree produced by :func:`qdk.openqasm.parser.parse` or the semantic tree
produced by :func:`qdk.openqasm.semantic.analyze`. Dispatch is by concrete node
type name, so the same visitor works across both layers: define a
``visit_<NodeType>`` method to handle a node kind, where ``<NodeType>`` is the
node's ``type(node).__name__`` (for example ``visit_QuantumGate`` in either
layer). Any node kind
without a matching method falls through to :meth:`generic_visit`, which recurses
over ``node.children()``.

This mirrors the ``visit``/``generic_visit`` contract of the ``openqasm3``
reference ``QASMVisitor``. An overriding ``visit_<NodeType>`` method should call
:meth:`generic_visit` itself when it wants traversal to continue into that
node's children::

    from qdk.openqasm import parser
    from qdk.openqasm.parser import QASMVisitor

    class GateCounter(QASMVisitor):
        def __init__(self) -> None:
            self.count = 0

        def visit_QuantumGate(self, node: object) -> None:
            self.count += 1
            self.generic_visit(node)

    result = parser.parse("OPENQASM 3.0; qubit q; x q; y q;")
    counter = GateCounter()
    counter.visit(result.program)
    assert counter.count == 2

The tree is immutable, so this visitor is read-only: it observes nodes but does
not rewrite them.
"""

from __future__ import annotations

from typing import Any


class QASMVisitor:
    """Read-only visitor base for the syntactic and semantic OpenQASM trees.

    Subclass and define ``visit_<NodeType>`` methods to handle specific node
    kinds, where ``<NodeType>`` matches ``type(node).__name__``. The default
    :meth:`generic_visit` recurses over every child returned by
    ``node.children()``.
    """

    def visit(self, node: Any) -> Any:
        """Dispatch to ``visit_<type(node).__name__>`` or :meth:`generic_visit`."""
        method = getattr(self, f"visit_{type(node).__name__}", self.generic_visit)
        return method(node)

    def generic_visit(self, node: Any) -> None:
        """Recurse over ``node.children()`` without modifying the tree."""
        for annotation in getattr(node, "annotations", ()):
            self.visit(annotation)
        for child in node.children():
            self.visit(child)
        return None
