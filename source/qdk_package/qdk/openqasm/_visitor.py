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

import inspect
from typing import Any, MutableMapping, Optional
from weakref import WeakKeyDictionary

_SENTINEL = object()

# `inspect.signature` costs tens of microseconds, so resolve callback arity once
# per callback function rather than once per visited node.
_ACCEPTS_CONTEXT: MutableMapping[Any, Optional[bool]] = WeakKeyDictionary()


def _accepts_context(method: Any) -> Optional[bool]:
    """Report whether ``method`` can be called with both a node and a context.

    The cache is keyed on the underlying function rather than on the bound
    method or the visitor, because arity is a property of the function: every
    visitor instance resolving to the same ``visit_<NodeType>`` shares one
    entry, while a subclass or per-instance override gets its own. Keys are held
    weakly so a callback defined in a short-lived scope is not pinned.

    Args:
        method: The resolved callback. Usually a bound ``visit_<NodeType>``
            method, but any callable assigned to the visitor is supported.

    Returns:
        ``True`` when the callback takes a context argument, ``False`` when it
        takes the node alone, or ``None`` when it exposes no introspectable
        signature and the caller must decide from the context it holds.
    """
    key = getattr(method, "__func__", method)
    try:
        return _ACCEPTS_CONTEXT[key]
    except (KeyError, TypeError):  # TypeError: key is not weak referenceable.
        pass

    accepts: Optional[bool]
    try:
        # Only arity is under test here, so the bound values are irrelevant.
        inspect.signature(method).bind(_SENTINEL, _SENTINEL)
        accepts = True
    except TypeError:
        accepts = False
    except ValueError:
        accepts = None

    try:
        _ACCEPTS_CONTEXT[key] = accepts
    except TypeError:
        pass
    return accepts


class QASMVisitor:
    """Read-only visitor base for the syntactic and semantic OpenQASM trees.

    Subclass and define ``visit_<NodeType>`` methods to handle specific node
    kinds, where ``<NodeType>`` matches ``type(node).__name__``. The default
    :meth:`generic_visit` recurses over every child returned by
    ``node.children()``.
    """

    def visit(self, node: Any, context: Any = None) -> Any:
        """Dispatch to a node callback, optionally carrying traversal context."""
        method = getattr(self, f"visit_{type(node).__name__}", self.generic_visit)
        accepts = _accepts_context(method)
        if accepts is False or (accepts is None and context is None):
            return method(node)
        return method(node, context)

    def generic_visit(self, node: Any, context: Any = None) -> None:
        """Recurse over every child with the same context.

        Statement nodes report their annotations from ``children()``, ahead of
        their own child nodes, so annotations need no separate traversal here.
        """
        for child in node.children():
            self.visit(child, context)
        return None
