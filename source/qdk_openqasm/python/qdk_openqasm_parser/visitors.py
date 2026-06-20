# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Visitor and rewriter idioms for the ``qdk_openqasm_parser`` node hierarchy.

These pure-Python classes sit on top of the native node classes exposed by
``._native``. Dispatch is by concrete node type name, so a ``visit_<NodeType>``
method (for example ``visit_ClassicalDeclStmt``) is invoked when present, and
``generic_visit`` recurses over ``node.children()`` otherwise.
"""

from __future__ import annotations

from typing import Any

from . import _native


class SyntaxVisitor:
    """Base visitor for the syntactic AST.

    Subclass and define ``visit_<NodeType>`` methods to handle specific node
    kinds. The default ``generic_visit`` recurses over every child node.
    """

    def visit(self, node: Any) -> Any:
        method = getattr(self, f"visit_{type(node).__name__}", self.generic_visit)
        return method(node)

    def generic_visit(self, node: Any) -> Any:
        for child in node.children():
            self.visit(child)
        return None


class SemanticVisitor:
    """Base visitor for the read-only semantic AST.

    Mirrors :class:`SyntaxVisitor`, dispatching on the concrete semantic node
    type name (for example ``visit_SemClassicalDeclStmt``).
    """

    def visit(self, node: Any) -> Any:
        method = getattr(self, f"visit_{type(node).__name__}", self.generic_visit)
        return method(node)

    def generic_visit(self, node: Any) -> Any:
        for child in node.children():
            self.visit(child)
        return None


class SyntaxRewriter(SyntaxVisitor):
    """A :class:`SyntaxVisitor` specialized for tree traversal and re-emission.

    ``rewrite`` walks the program, and ``unparse`` delegates to the native
    emitter to produce canonical ``OpenQASM 3`` source for a program.
    """

    def rewrite(self, program: Any) -> Any:
        self.visit(program)
        return program

    def unparse(self, program: Any) -> str:
        return _native.unparse(program)


__all__ = [
    "SyntaxVisitor",
    "SemanticVisitor",
    "SyntaxRewriter",
]
