# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Guards that public docstrings read as Python, not as Rust.

Every docstring on the node surface is authored in Rust and reaches users
verbatim through ``help()`` and IDE hovers. A rustdoc intra-doc link, a Rust
path separator, or a private ``Sem``-prefixed native name means nothing to a
Python reader. The stub-parity guard cannot catch any of them, because it
compares the two copies of a docstring to each other rather than to a standard.
"""

from __future__ import annotations

import inspect
import re
from typing import Any, Iterator

from qdk.openqasm import parser, semantic, source

# Rust-only documentation idioms. Rust renders the first two as links; Python
# renders them literally. The third is a private native identifier that the
# un-prefixed public naming scheme exists to keep out of user-facing text.
_RUST_IDIOMS = {
    "rustdoc intra-doc link": re.compile(r"\[`{1,2}[^`\]]+`{1,2}\]"),
    "Rust path separator": re.compile(r"\w::\w"),
    "private Sem-prefixed name": re.compile(r"\bSem[A-Z]\w*"),
}

_SWEPT_MODULES = (parser, semantic, source)


def _rust_idiom_hits(label: str, doc: str | None) -> list[str]:
    if not doc:
        return []
    return [
        f"{label}: {idiom} in {doc.strip().splitlines()[0]!r}"
        for idiom, pattern in _RUST_IDIOMS.items()
        if pattern.search(doc)
    ]


def _own_members(cls: type) -> Iterator[tuple[str, Any]]:
    """Members the class itself documents, skipping those inherited from ``object``."""
    for name in dir(cls):
        attr = inspect.getattr_static(cls, name, None)
        if attr is None or attr is getattr(object, name, None):
            continue
        yield name, attr


def _public_docstrings(module: Any) -> Iterator[tuple[str, str | None]]:
    """Every docstring reachable through ``help()`` on a public layer module.

    Results, symbols, diagnostics, and the module-level entry points are swept
    alongside the nodes: they are the text a user reads first.
    """
    yield module.__name__, module.__doc__
    for name in module.__all__:
        value = getattr(module, name)
        if isinstance(value, type):
            yield f"{module.__name__}.{name}", value.__doc__
            for member, attr in _own_members(value):
                yield (
                    f"{module.__name__}.{name}.{member}",
                    getattr(attr, "__doc__", None),
                )
        elif callable(value):
            yield f"{module.__name__}.{name}", value.__doc__


def test_no_rust_doc_idiom_leaks_into_any_public_docstring() -> None:
    leaks = [
        hit
        for module in _SWEPT_MODULES
        for label, doc in _public_docstrings(module)
        for hit in _rust_idiom_hits(label, doc)
    ]
    assert not leaks, "Rust documentation idioms in public docstrings:\n" + "\n".join(
        leaks
    )


def test_the_idiom_patterns_flag_rust_prose() -> None:
    """Without this, a broken pattern would pass the guard above vacuously."""
    for injected in (
        "The result of a syntactic [`parse`].",
        "Alias for [`ParseResult::diagnostics`].",
        "The symbol's unique id within the [``SemSymbolTable``].",
        "Builds the semantic SemProgram root from an analyzed program.",
    ):
        assert _rust_idiom_hits("injected", injected), f"sweep missed {injected!r}"


def test_the_idiom_patterns_leave_python_prose_alone() -> None:
    """Sphinx roles, double-backtick literals, and OpenQASM syntax are not leaks."""
    for benign in (
        "The result of a syntactic :func:`parse`.",
        "A quantum gate modifier (for example ``ctrl @`` or ``pow(2) @``).",
        "A hardware-qubit gate operand (for example ``$0``).",
        "The semantic layer's ``SemanticExpression`` base, not a private name.",
        "Indexes are zero based, so ``qubits[0]`` is the first operand.",
    ):
        assert not _rust_idiom_hits("benign", benign), f"sweep tripped on {benign!r}"
