# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Guards that the type stub documents the node surface as well as the runtime.

``qdk._native`` is a compiled extension. Type checkers and editors treat its
``.pyi`` stub as authoritative and have no Python source to recover a docstring
from, so a runtime docstring alone never reaches hover or completion. The stub
therefore repeats the text authored in Rust, and these tests make the two
diverging a test failure rather than a slow decay.
"""

from __future__ import annotations

import ast
import pathlib
from typing import Any, Iterator

from qdk.openqasm import parser, semantic

_STUB = pathlib.Path(__file__).resolve().parents[1] / "qdk" / "_native.pyi"

# Documented on the base class that declares them, not on every subclass.
_INHERITED = {"span", "annotations", "ty", "const_value", "symbol", "name", "is_const"}
_AUXILIARY_CLASSES = {
    "syntax": {
        "RangeDefinition",
        "DiscreteSet",
        "IndexList",
        "SwitchCase",
        "SubroutineParameter",
    },
    "semantic": {
        "QuantumGateModifier",
        "RangeDefinition",
        "DiscreteSet",
        "SwitchCase",
        "SubroutineParameter",
        "GateParameter",
    },
}


def _stub_module() -> ast.Module:
    return ast.parse(_STUB.read_text(encoding="utf-8"))


def _class_defs(node: ast.Module | ast.ClassDef) -> dict[str, ast.ClassDef]:
    return {n.name: n for n in node.body if isinstance(n, ast.ClassDef)}


def _stub_classes() -> dict[str, dict[str, ast.ClassDef]]:
    """Maps a Python class name to its stub definition, per layer.

    Semantic classes live in a nested `_semantic` namespace, matching the
    private native submodule they are registered in. A few classes are shared
    by both layers and are declared once at the top level, so the semantic
    lookup falls back there.
    """
    top = _class_defs(_stub_module())
    semantic_ns = top.pop("_semantic", None)
    nested = _class_defs(semantic_ns) if semantic_ns else {}
    return {"syntax": top, "semantic": {**top, **nested}}


def _stub_properties(cls: ast.ClassDef) -> dict[str, str | None]:
    return {
        item.name: ast.get_docstring(item)
        for item in cls.body
        if isinstance(item, ast.FunctionDef)
        and any(
            isinstance(d, ast.Name) and d.id == "property" for d in item.decorator_list
        )
    }


def _comparable_docstring(docstring: str) -> str:
    """Normalize Rustdoc and reStructuredText inline-code delimiters."""
    return " ".join(docstring.replace("``", "`").split())


def _property_callable_mismatches(
    stub: dict[str, dict[str, ast.ClassDef]],
) -> list[str]:
    """Find stub properties whose runtime member is callable instead."""
    problems: list[str] = []
    for layer, module in (("syntax", parser), ("semantic", semantic)):
        for class_name, cls in _runtime_node_classes(module):
            stub_cls = stub[layer].get(class_name)
            if stub_cls is None:
                continue
            for accessor in _stub_properties(stub_cls):
                if callable(getattr(cls, accessor, None)):
                    problems.append(f"{layer}.{class_name}.{accessor}")
    return problems


def _runtime_node_classes(module: Any) -> Iterator[tuple[str, type]]:
    """Every class the stub must mirror.

    Resolved semantic types are included even though they are not ``QASMNode``
    subclasses, because they are part of the documented surface.
    """
    roots = (parser.QASMNode, semantic.Type)
    for name in module.__all__:
        value = getattr(module, name, None)
        if isinstance(value, type) and issubclass(value, roots):
            yield name, value


def _runtime_non_node_classes(
    module: Any,
    stub_layer: dict[str, ast.ClassDef],
) -> Iterator[tuple[str, type]]:
    """Native public classes outside the syntax-node and resolved-type trees.

    Python-authored public helpers have no native stub definition. Filtering by
    the parsed stub deliberately excludes them while keeping the boundary tied
    to the module's public export registry.
    """
    roots = (parser.QASMNode, semantic.Type)
    for name in module.__all__:
        value = getattr(module, name, None)
        if (
            isinstance(value, type)
            and not issubclass(value, roots)
            and name in stub_layer
        ):
            yield name, value


def _runtime_accessors(cls: type) -> dict[str, str | None]:
    return {
        name: getattr(getattr(cls, name), "__doc__", None)
        for name in vars(cls)
        if not name.startswith("_") and name != "children" and name not in _INHERITED
    }


def _mismatches() -> list[str]:
    stub = _stub_classes()
    problems: list[str] = []
    for layer, module in (("syntax", parser), ("semantic", semantic)):
        stub_layer = stub[layer]
        for class_name, cls in _runtime_node_classes(module):
            stub_cls = stub_layer.get(class_name)
            if stub_cls is None:
                problems.append(f"{layer}.{class_name}: missing from the stub")
                continue
            stub_props = _stub_properties(stub_cls)
            for accessor, runtime_doc in _runtime_accessors(cls).items():
                if accessor not in stub_props:
                    problems.append(
                        f"{layer}.{class_name}.{accessor}: missing from the stub"
                    )
                elif stub_props[accessor] != runtime_doc:
                    problems.append(
                        f"{layer}.{class_name}.{accessor}: stub says "
                        f"{stub_props[accessor]!r}, runtime says {runtime_doc!r}"
                    )
    return problems


def _non_node_doc_mismatches(
    stub: dict[str, dict[str, ast.ClassDef]] | None = None,
) -> list[str]:
    """Find undocumented or diverging native public non-node members."""
    if stub is None:
        stub = _stub_classes()
    problems: list[str] = []
    for layer, module in (("syntax", parser), ("semantic", semantic)):
        for class_name, cls in _runtime_non_node_classes(module, stub[layer]):
            stub_cls = stub[layer][class_name]
            if ast.get_docstring(stub_cls) is None:
                problems.append(f"{layer}.{class_name}: missing class doc")
            for accessor, stub_doc in _stub_properties(stub_cls).items():
                if stub_doc is None:
                    problems.append(
                        f"{layer}.{class_name}.{accessor}: missing property doc"
                    )
                    continue
                runtime_doc = getattr(getattr(cls, accessor, None), "__doc__", None)
                if (
                    runtime_doc is not None
                    and _comparable_docstring(runtime_doc)
                    != _comparable_docstring(stub_doc)
                ):
                    problems.append(
                        f"{layer}.{class_name}.{accessor}: stub says "
                        f"{stub_doc!r}, runtime says {runtime_doc!r}"
                    )
    return problems


def test_stub_property_docstrings_match_the_runtime() -> None:
    problems = _mismatches()
    assert not problems, (
        f"{len(problems)} stub/runtime documentation mismatches:\n"
        + "\n".join(problems)
    )


def test_stub_properties_are_not_runtime_methods() -> None:
    problems = _property_callable_mismatches(_stub_classes())
    assert not problems, f"stub properties implemented as methods: {problems}"


def test_property_callable_guard_rejects_a_mutated_stub() -> None:
    stub = _stub_classes()
    extern = stub["syntax"]["ExternDeclaration"]
    children = next(
        item
        for item in extern.body
        if isinstance(item, ast.FunctionDef) and item.name == "children"
    )
    children.decorator_list.append(ast.Name(id="property"))

    assert _property_callable_mismatches(stub) == [
        "syntax.ExternDeclaration.children"
    ]


def test_every_node_class_has_a_stub_class_docstring() -> None:
    stub = _stub_classes()
    missing = [
        f"{layer}.{class_name}"
        for layer, module in (("syntax", parser), ("semantic", semantic))
        for class_name, _ in _runtime_node_classes(module)
        if (cls := stub[layer].get(class_name)) is not None
        and ast.get_docstring(cls) is None
    ]
    assert not missing, f"node classes without a stub docstring: {missing}"


def test_auxiliary_node_class_docstrings_match_the_runtime() -> None:
    stub = _stub_classes()
    problems = []
    for layer, module in (("syntax", parser), ("semantic", semantic)):
        for class_name in _AUXILIARY_CLASSES[layer]:
            runtime_doc = getattr(module, class_name).__doc__
            stub_doc = ast.get_docstring(stub[layer][class_name])
            if runtime_doc != stub_doc:
                problems.append(
                    f"{layer}.{class_name}: stub says {stub_doc!r}, runtime says {runtime_doc!r}"
                )
    assert not problems, "\n".join(problems)


def test_non_node_stub_documentation_matches_public_exports() -> None:
    problems = _non_node_doc_mismatches()
    assert not problems, "\n".join(problems)


def test_non_node_documentation_guard_rejects_a_mutated_stub() -> None:
    stub = _stub_classes()
    symbol = stub["semantic"]["Symbol"]
    name = next(
        item
        for item in symbol.body
        if isinstance(item, ast.FunctionDef) and item.name == "name"
    )
    name.body.pop(0)

    assert _non_node_doc_mismatches(stub) == [
        "semantic.Symbol.name: missing property doc"
    ]


def test_the_stub_sweep_actually_covers_the_surface() -> None:
    """A guard that would otherwise pass vacuously if stub parsing broke."""
    stub = _stub_classes()
    assert len(stub["syntax"]) > 100, f"parsed only {len(stub['syntax'])} stub classes"
    assert len(stub["semantic"]) > 50, (
        f"parsed only {len(stub['semantic'])} nested stub classes"
    )
    checked = sum(
        len(_runtime_accessors(cls))
        for module in (parser, semantic)
        for _, cls in _runtime_node_classes(module)
    )
    assert checked >= 150, f"only {checked} accessors compared"
