# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Guards that the node surface documents itself.

``qdk.openqasm`` exposes well over a hundred node classes. If their accessors
carry no docstring, ``help()`` and IDE hovers show nothing and the surface is
undiscoverable. These tests enumerate classes from the native registry rather
than a hard-coded list, so a class added later is covered automatically.
"""

from __future__ import annotations

from typing import Any, Iterator

from qdk.openqasm import parser, semantic

# Abstract bases exist only for `isinstance` dispatch and declare no fields of
# their own beyond those documented on the class they inherit from.
_ABSTRACT = {
    "QASMNode",
    "Expression",
    "Statement",
    "ClassicalType",
    "SemanticExpression",
    "SemanticStatement",
    "Type",
}

# Inherited from a base class, which documents them once.
_INHERITED = {"span", "annotations", "ty", "const_value", "symbol", "name", "is_const"}

# Traversal and rendering protocol, not data accessors.
_PROTOCOL = {"children"}


def _registered_node_classes(module: Any) -> Iterator[tuple[str, type]]:
    """Every documented class in the registry.

    Resolved semantic types are covered too. They are deliberately not
    ``QASMNode`` subclasses, because a resolved type has no source position, so
    a ``QASMNode``-only sweep would leave the whole type family unguarded.
    """
    roots = (parser.QASMNode, semantic.Type)
    for name in module.__all__:
        value = getattr(module, name, None)
        if isinstance(value, type) and issubclass(value, roots):
            yield name, value


def _documentable_accessors(cls: type) -> Iterator[str]:
    for name in vars(cls):
        if name.startswith("_") or name in _PROTOCOL:
            continue
        yield name


def _undocumented(module: Any) -> list[str]:
    missing: list[str] = []
    for class_name, cls in _registered_node_classes(module):
        if class_name in _ABSTRACT:
            continue
        for accessor in _documentable_accessors(cls):
            if accessor in _INHERITED:
                continue
            doc = getattr(getattr(cls, accessor), "__doc__", None)
            if not doc:
                missing.append(f"{module.__name__}.{class_name}.{accessor}")
    return missing


def test_every_syntax_accessor_is_documented() -> None:
    missing = _undocumented(parser)
    assert not missing, "undocumented accessors:\n" + "\n".join(missing)


def test_every_semantic_accessor_is_documented() -> None:
    missing = _undocumented(semantic)
    assert not missing, "undocumented accessors:\n" + "\n".join(missing)


def test_the_registry_sweep_actually_covers_the_surface() -> None:
    """A guard that would otherwise pass vacuously if enumeration broke."""
    syntax = list(_registered_node_classes(parser))
    sem = list(_registered_node_classes(semantic))
    assert len(syntax) >= 75, f"only found {len(syntax)} syntax node classes"
    assert len(sem) >= 55, f"only found {len(sem)} semantic node classes"

    accessors = sum(
        len([a for a in _documentable_accessors(cls) if a not in _INHERITED])
        for _, cls in syntax + sem
    )
    assert accessors >= 150, f"only found {accessors} accessors to check"


def test_documented_accessors_read_naturally() -> None:
    """Spot-check that docstrings describe the field, not the implementation."""
    assert parser.Identifier.name.__doc__ == "The identifier's source text."
    assert parser.QuantumGate.qubits.__doc__ == "The qubit operands the gate acts on."
    assert (
        parser.BitstringLiteral.width.__doc__
        == "The number of bits written in the source, including leading zeros."
    )
    assert semantic.QuantumGate.name.__doc__ == (
        "The gate's name, when analysis resolved one."
    )


def test_every_node_class_has_a_class_docstring() -> None:
    undocumented = [
        f"{module.__name__}.{name}"
        for module in (parser, semantic)
        for name, cls in _registered_node_classes(module)
        if not cls.__doc__
    ]
    assert not undocumented, f"classes without a docstring: {undocumented}"
