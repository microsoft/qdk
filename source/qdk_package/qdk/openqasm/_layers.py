# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Virtual base classes that identify which OpenQASM tree a class belongs to.

The two AST layers give the same name to different classes: a majority of the
classes exported from :mod:`qdk.openqasm.parser` have a same-named counterpart
in :mod:`qdk.openqasm.semantic`. A value named ``Program``, ``QuantumGate``, or
``IntType`` therefore says nothing on its own about which tree produced it.
Layer membership is visible in each class's ``__module__``, but that is a naming
convention rather than a supported check.

:class:`SyntaxNode` and :class:`SemanticNode` make it supported. They are
:mod:`abc` bases with no members; each layer registers its own tree classes as
virtual subclasses at import time, so ``isinstance`` answers the question
without any change to the native inheritance chain and without per-node cost.

This module deliberately imports nothing from either layer. Registration is
driven the other way around: :mod:`~qdk.openqasm.parser` and
:mod:`~qdk.openqasm.semantic` each call :func:`register_layer` for the family
they own, so neither module has to import the other.
"""

from __future__ import annotations

import abc
from typing import Any, Dict, Iterable, Tuple


class SyntaxNode(abc.ABC):
    """Virtual base of every class in the syntactic tree.

    ``isinstance(node, SyntaxNode)`` is true for nodes produced by
    :func:`qdk.openqasm.parser.parse` and false for nodes produced by
    :func:`qdk.openqasm.semantic.analyze`, with the shared-class exception
    described in :data:`SHARED_CLASS_NAMES`.

    This class is never instantiated or subclassed directly. Membership is
    virtual, so it resolves through ``ABCMeta.__instancecheck__``: appropriate
    for a diagnostic check at an API boundary, not for a hot traversal loop.
    """

    # Declared where users import it from rather than where it is defined, as
    # the shared node classes are.
    __module__ = "qdk.openqasm.parser"
    __slots__ = ()


class SemanticNode(abc.ABC):
    """Virtual base of every class in the semantic tree.

    The counterpart of :class:`SyntaxNode`. It also covers the resolved types
    rooted at :class:`qdk.openqasm.semantic.Type`, which are not
    :class:`~qdk.openqasm.QASMNode` instances and carry no span, but are part
    of the semantic layer's vocabulary and collide by name with the syntactic
    type nodes.
    """

    __module__ = "qdk.openqasm.semantic"
    __slots__ = ()


#: The classes both layers use, which belong to neither family.
#:
#: ``QASMNode``, ``Expression``, and ``Statement`` are bases that both trees
#: inherit; ``Annotation`` is one concrete class that both trees instantiate.
#: Registering them to both families would make each predicate answer ``True``
#: to both questions and so mean nothing, and registering them to one family
#: would be false. They therefore satisfy neither predicate: asking which tree
#: a shared class came from is a question with no answer.
SHARED_CLASS_NAMES: Tuple[str, ...] = (
    "QASMNode",
    "Expression",
    "Statement",
    "Annotation",
)


def register_layer(
    family: type,
    namespace: Dict[str, Any],
    exported: Iterable[str],
    roots: Tuple[type, ...],
) -> None:
    """Register a layer's tree classes as virtual subclasses of ``family``.

    Called once by each layer module after its ``__all__`` is bound. Driving
    registration from the exported inventory rather than a hand-written list
    means a new node kind cannot be added without joining its family.

    Args:
        family: :class:`SyntaxNode` or :class:`SemanticNode`.
        namespace: The calling module's ``globals()``.
        exported: The calling module's ``__all__``.
        roots: The tree roots for this layer. A class counts as a tree class
            when it derives from one of them, which is the same test the
            package's hierarchy-sweeping tests use.
    """
    for name in exported:
        if name in SHARED_CLASS_NAMES:
            continue
        candidate = namespace.get(name)
        if isinstance(candidate, type) and issubclass(candidate, roots):
            family.register(candidate)
