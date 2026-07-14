# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Guards that the analysis projection shares its repeated objects.

A resolved type and a resolved symbol are pure functions of the analysis data
they project, but the projection used to rebuild them per occurrence: a
32,003-node program held 26,001 type objects for 5 distinct values and 8,000
symbol objects for 1 distinct id. That cost roughly 35 percent in retained
memory per node.

These tests assert object sharing rather than a byte figure. Sharing is
deterministic and is the property that was actually wrong; a byte threshold is
environment-sensitive and belongs in a recorded measurement instead.
"""

from __future__ import annotations

from typing import Any, List

from qdk.openqasm import semantic

_SOURCE = (
    'OPENQASM 3.0;\ninclude "stdgates.inc";\nqubit[4] q;\n'
    + "h q[0];\ncx q[0], q[1];\nrz(0.5) q[2];\n" * 200
)


def _walk(node: Any) -> Any:
    yield node
    for child in node.children():
        yield from _walk(child)


def _analyze(source: str = _SOURCE) -> Any:
    result = semantic.analyze(source)
    assert not result.has_errors
    return result


def test_equal_resolved_types_share_one_object() -> None:
    program = _analyze().program
    objects: set[int] = set()
    values: set[str] = set()
    for node in _walk(program):
        resolved = getattr(node, "ty", None)
        if resolved is not None:
            objects.add(id(resolved))
            values.add(repr(resolved))

    assert values, "corpus produced no typed expressions"
    assert len(objects) == len(values), (
        f"{len(objects)} type objects for {len(values)} distinct values; "
        "resolved types are no longer interned"
    )


def test_repeated_symbol_references_share_one_object() -> None:
    program = _analyze().program
    objects: set[int] = set()
    ids: set[int] = set()
    for node in _walk(program):
        symbol = getattr(node, "symbol", None)
        if symbol is not None:
            objects.add(id(symbol))
            ids.add(symbol.id)

    assert ids, "corpus produced no resolved symbols"
    assert len(objects) == len(ids), (
        f"{len(objects)} symbol objects for {len(ids)} distinct ids; "
        "symbol projections are no longer interned"
    )


def test_the_symbol_table_shares_objects_with_the_tree() -> None:
    """One context spans both projections, so they must not build separate objects."""
    result = _analyze()
    from_tree = {
        id(symbol)
        for node in _walk(result.program)
        if (symbol := getattr(node, "symbol", None)) is not None
    }
    from_table = {id(symbol) for symbol in result.symbols}
    assert from_tree, "corpus produced no resolved symbols"
    assert from_tree <= from_table, (
        "the tree holds symbol objects the symbol table does not; "
        "the two projections are not sharing one cache"
    )


def test_interning_does_not_change_observable_equality() -> None:
    """Sharing is an implementation detail; two analyses must still compare equal."""
    first = _analyze().program
    second = _analyze().program
    assert first is not second
    assert first == second
    assert hash(first) == hash(second)


def test_the_sharing_sweep_actually_covers_the_surface() -> None:
    """A guard that would otherwise pass vacuously if traversal broke."""
    nodes: List[Any] = list(_walk(_analyze().program))
    assert len(nodes) > 2000, f"corpus only produced {len(nodes)} nodes"
