# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Guards that every exported node class is producible from OpenQASM source.

Nothing in the build previously connected the exported class list to what the
parser and lowerer actually construct, so classes that no source could ever
produce shipped undetected. These tests parse and analyze the shared corpus in
``qasm_corpus``, walk the resulting trees, and require every exported concrete
class to appear. Anything that cannot appear must be listed in
``qasm_corpus.UNPRODUCIBLE`` with a stated reason.
"""

from __future__ import annotations

from typing import Any, Iterator

from qasm_corpus import ABSTRACT as _ABSTRACT
from qasm_corpus import CORPUS as _CORPUS
from qasm_corpus import SOURCES_WITH_EXPECTED_ERRORS as _SOURCES_WITH_EXPECTED_ERRORS
from qasm_corpus import UNPRODUCIBLE as _UNPRODUCIBLE
from qdk.openqasm import parser, semantic


def _concrete_exports(module: Any) -> set[str]:
    names: set[str] = set()
    for name in module.__all__:
        value = getattr(module, name, None)
        if (
            isinstance(value, type)
            and issubclass(value, parser.QASMNode)
            and name not in _ABSTRACT
        ):
            names.add(name)
    return names


def _walk(node: Any, seen: set[str]) -> None:
    seen.add(type(node).__name__)
    # Annotations are reported by `children()`, so no separate traversal.
    for child in node.children():
        _walk(child, seen)


def _errors(result: Any) -> list[str]:
    return [
        d.message for d in result.diagnostics if "Error" in str(d.severity)
    ]


def _produced() -> tuple[set[str], set[str]]:
    """Return the class names the corpus produces in the syntax and semantic layers."""
    syntax_seen: set[str] = set()
    semantic_seen: set[str] = set()
    for source in _CORPUS.values():
        parsed = parser.parse(source)
        if parsed.program is not None:
            _walk(parsed.program, syntax_seen)
        analyzed = semantic.analyze(source)
        if analyzed.program is not None:
            _walk(analyzed.program, semantic_seen)
    return syntax_seen, semantic_seen


def _missing(module: Any, seen: set[str]) -> list[str]:
    return sorted(_concrete_exports(module) - seen - set(_UNPRODUCIBLE))


def test_every_exported_syntax_class_is_producible() -> None:
    syntax_seen, _ = _produced()
    missing = _missing(parser, syntax_seen)
    assert not missing, (
        "these exported syntax classes are not produced by any corpus source. "
        "Either add a source that reaches them, or remove them from the exported "
        "surface:\n" + "\n".join(missing)
    )


def test_every_exported_semantic_class_is_producible() -> None:
    _, semantic_seen = _produced()
    missing = _missing(semantic, semantic_seen)
    assert not missing, (
        "these exported semantic classes are not produced by any corpus source. "
        "Either add a source that reaches them, or remove them from the exported "
        "surface:\n" + "\n".join(missing)
    )


def test_the_exemption_list_is_not_stale() -> None:
    """An exempted class that no longer exists, or is now reachable, must be re-examined."""
    exported = _concrete_exports(parser) | _concrete_exports(semantic)
    unknown = sorted(set(_UNPRODUCIBLE) - exported)
    assert not unknown, (
        "these names are exempted but no longer exported; drop them from "
        "_UNPRODUCIBLE:\n" + "\n".join(unknown)
    )


def test_the_corpus_stays_well_formed() -> None:
    """A source that starts failing to lower silently stops covering its classes."""
    unexpected: list[str] = []
    for name, source in _CORPUS.items():
        if name in _SOURCES_WITH_EXPECTED_ERRORS:
            continue
        for message in _errors(parser.parse(source)):
            unexpected.append(f"{name} (parse): {message}")
        for message in _errors(semantic.analyze(source)):
            unexpected.append(f"{name} (analyze): {message}")
    assert not unexpected, "corpus sources produced errors:\n" + "\n".join(unexpected)


def test_the_sources_with_expected_errors_still_error() -> None:
    """Otherwise the exemption hides a source that has silently become clean."""
    for name in _SOURCES_WITH_EXPECTED_ERRORS:
        assert name in _CORPUS, f"{name} is exempted but absent from the corpus"
        assert _errors(
            semantic.analyze(_CORPUS[name])
        ), f"{name} no longer errors; move it out of _SOURCES_WITH_EXPECTED_ERRORS"


def test_the_reachability_sweep_actually_covers_the_surface() -> None:
    """A guard that would otherwise pass vacuously if enumeration or traversal broke."""
    syntax_exports = _concrete_exports(parser)
    semantic_exports = _concrete_exports(semantic)
    assert len(syntax_exports) > 60, f"only found {len(syntax_exports)} syntax classes"
    assert len(semantic_exports) > 40, (
        f"only found {len(semantic_exports)} semantic classes"
    )

    syntax_seen, semantic_seen = _produced()
    assert len(syntax_seen & syntax_exports) > 60, (
        f"corpus only reached {len(syntax_seen & syntax_exports)} syntax classes"
    )
    assert len(semantic_seen & semantic_exports) > 40, (
        f"corpus only reached {len(semantic_seen & semantic_exports)} semantic classes"
    )
