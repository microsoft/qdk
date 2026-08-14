# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Guards that the type stub documents the node surface as well as the runtime.

``qdk._native`` is a compiled extension. Type checkers and editors treat its
``.pyi`` stub as authoritative and have no Python source to recover a docstring
from, so a runtime docstring alone never reaches hover or completion. The stub
therefore repeats the text authored in Rust, and these tests make the two
diverging a test failure rather than a slow decay.

The OpenQASM declarations live in sibling stubs next to the modules that consume
them, one per layer, rather than in the shared ``qdk/_native.pyi``.
"""

from __future__ import annotations

import ast
import importlib
import inspect
import pathlib
import re
from typing import Any, Iterator

import qdk.openqasm as openqasm

parser = importlib.import_module("qdk.openqasm.parser")
semantic = importlib.import_module("qdk.openqasm.semantic")

_QASM_PACKAGE = pathlib.Path(__file__).resolve().parents[1] / "qdk" / "openqasm"
_SYNTAX_STUB = _QASM_PACKAGE / "_native_syntax.pyi"
_SEMANTIC_STUB = _QASM_PACKAGE / "_native_semantic.pyi"
_NATIVE_STUB = _QASM_PACKAGE.parent / "_native.pyi"
_INTERPRETER_RS = _QASM_PACKAGE.parents[1] / "src" / "interpreter.rs"
_SHARED_NATIVE_CLASSES = ("OutputSemantics", "ProgramType", "QasmError")
_SHARED_NATIVE_ENUMS = ("OutputSemantics", "ProgramType")


def _stub_module(path: pathlib.Path) -> ast.Module:
    return ast.parse(path.read_text(encoding="utf-8"))


def _class_defs(node: ast.Module | ast.ClassDef) -> dict[str, ast.ClassDef]:
    return {n.name: n for n in node.body if isinstance(n, ast.ClassDef)}


def _stub_classes() -> dict[str, dict[str, ast.ClassDef]]:
    """Maps a Python class name to its stub definition, per layer.

    Each layer has its own stub. A few classes are shared by both layers and are
    declared once in the syntax stub, so the semantic lookup falls back there.
    """
    syntax = _class_defs(_stub_module(_SYNTAX_STUB))
    semantic_only = _class_defs(_stub_module(_SEMANTIC_STUB))
    return {"syntax": syntax, "semantic": {**syntax, **semantic_only}}


def _native_stub_classes() -> dict[str, ast.ClassDef]:
    return _class_defs(_stub_module(_NATIVE_STUB))


def _stub_enum_member_docs(cls: ast.ClassDef) -> dict[str, str]:
    docs: dict[str, str] = {}
    for index, item in enumerate(cls.body[:-1]):
        following = cls.body[index + 1]
        if (
            isinstance(item, ast.AnnAssign)
            and isinstance(item.target, ast.Name)
            and isinstance(following, ast.Expr)
            and isinstance(following.value, ast.Constant)
            and isinstance(following.value.value, str)
        ):
            docs[item.target.id] = inspect.cleandoc(following.value.value)
    return docs


def _rust_enum_member_docs(enum_name: str) -> dict[str, str]:
    source = _INTERPRETER_RS.read_text(encoding="utf-8")
    match = re.search(
        rf"\b(?:pub\(crate\)|pub) enum {enum_name} \{{(.*?)\n\}}", source, re.S
    )
    assert match is not None, f"Rust enum {enum_name} not found"

    docs: dict[str, str] = {}
    pending: list[str] = []
    for line in match.group(1).splitlines():
        stripped = line.strip()
        if stripped.startswith("///"):
            pending.append(stripped.removeprefix("///").removeprefix(" "))
        elif variant := re.fullmatch(r"(\w+),", stripped):
            docs[variant.group(1)] = "\n".join(pending)
            pending = []
        elif stripped and not stripped.startswith("#["):
            pending = []
    return docs


def _stub_properties(cls: ast.ClassDef) -> dict[str, str | None]:
    return {
        item.name: ast.get_docstring(item)
        for item in cls.body
        if isinstance(item, ast.FunctionDef)
        and any(
            isinstance(d, ast.Name) and d.id == "property" for d in item.decorator_list
        )
    }


def _stub_public_methods(cls: ast.ClassDef) -> dict[str, str | None]:
    return {
        item.name: ast.get_docstring(item)
        for item in cls.body
        if isinstance(item, ast.FunctionDef)
        and not item.name.startswith("_")
        and item.name != "children"
        and not any(
            isinstance(decorator, ast.Name) and decorator.id == "property"
            for decorator in item.decorator_list
        )
    }


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
        if not name.startswith("_") and name != "children"
    }


def _class_doc_mismatches(
    stub: dict[str, dict[str, ast.ClassDef]] | None = None,
) -> list[str]:
    """Find exported native classes whose runtime and stub docs differ."""
    if stub is None:
        stub = _stub_classes()
    problems: list[str] = []
    for layer, module in (("syntax", parser), ("semantic", semantic)):
        for class_name in module.__all__:
            cls = getattr(module, class_name, None)
            stub_cls = stub[layer].get(class_name)
            if not isinstance(cls, type) or stub_cls is None:
                continue
            stub_doc = ast.get_docstring(stub_cls)
            runtime_doc = cls.__doc__
            if stub_doc != runtime_doc:
                problems.append(
                    f"{layer}.{class_name}: stub says {stub_doc!r}, "
                    f"runtime says {runtime_doc!r}"
                )
    return problems


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
                if runtime_doc is not None and runtime_doc != stub_doc:
                    problems.append(
                        f"{layer}.{class_name}.{accessor}: stub says "
                        f"{stub_doc!r}, runtime says {runtime_doc!r}"
                    )
            for method, stub_doc in _stub_public_methods(stub_cls).items():
                if stub_doc is None:
                    problems.append(
                        f"{layer}.{class_name}.{method}: missing method doc"
                    )
                    continue
                runtime_doc = getattr(getattr(cls, method, None), "__doc__", None)
                if runtime_doc is None:
                    problems.append(
                        f"{layer}.{class_name}.{method}: missing runtime method doc"
                    )
                elif runtime_doc != stub_doc:
                    problems.append(
                        f"{layer}.{class_name}.{method}: stub says "
                        f"{stub_doc!r}, runtime says {runtime_doc!r}"
                    )
    return problems


def test_stub_property_docstrings_match_the_runtime() -> None:
    problems = _mismatches()
    assert (
        not problems
    ), f"{len(problems)} stub/runtime documentation mismatches:\n" + "\n".join(problems)


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

    assert _property_callable_mismatches(stub) == ["syntax.ExternDeclaration.children"]


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


def test_all_native_class_docstrings_match_the_runtime() -> None:
    problems = _class_doc_mismatches()
    assert not problems, "\n".join(problems)


def test_shared_native_class_docstrings_match_the_runtime() -> None:
    stub = _native_stub_classes()
    problems = [
        f"{name}: stub says {ast.get_docstring(stub[name])!r}, "
        f"runtime says {getattr(openqasm, name).__doc__!r}"
        for name in _SHARED_NATIVE_CLASSES
        if ast.get_docstring(stub[name]) != getattr(openqasm, name).__doc__
    ]
    assert not problems, "\n".join(problems)


def test_shared_native_enum_member_docs_match_rust() -> None:
    stub = _native_stub_classes()
    problems = []
    for enum_name in _SHARED_NATIVE_ENUMS:
        stub_docs = _stub_enum_member_docs(stub[enum_name])
        rust_docs = _rust_enum_member_docs(enum_name)
        for member, stub_doc in stub_docs.items():
            rust_doc = rust_docs.get(member)
            if stub_doc != rust_doc:
                problems.append(
                    f"{enum_name}.{member}: stub says {stub_doc!r}, "
                    f"Rust says {rust_doc!r}"
                )
    assert not problems, "\n".join(problems)


def test_class_documentation_guard_rejects_a_mutated_stub() -> None:
    stub = _stub_classes()
    identifier = stub["syntax"]["Identifier"]
    docstring = identifier.body[0]
    assert isinstance(docstring, ast.Expr)
    docstring.value = ast.Constant(value="Changed documentation.")

    assert _class_doc_mismatches(stub) == [
        "syntax.Identifier: stub says 'Changed documentation.', "
        f"runtime says {parser.Identifier.__doc__!r}"
    ]


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
