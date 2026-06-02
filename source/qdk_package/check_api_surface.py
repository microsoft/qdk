#!/usr/bin/env python3

# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Lint: detect private-symbol leakage in the qdk public API surface.

For every symbol listed in a module's ``__all__``, this script inspects
function / method type annotations.  If any annotation references a type
that is **only** reachable through an underscore-prefixed (private)
module path, a violation is reported.

Base classes are intentionally *not* checked — a private base (mixin,
ABC, protocol) is an implementation detail that users never need to
reference directly, so it does not constitute actionable leakage.

Types that are defined in a private module but re-exported through a
public module's ``__all__`` are **not** flagged — they are considered
public.

Exit code 0  - no violations found.
Exit code 1  - one or more violations found (details printed to stderr).

Usage::

    python check_api_surface.py          # check everything
    python check_api_surface.py --json   # machine-readable output
"""

from __future__ import annotations

import argparse
import importlib
import inspect
import json
import pkgutil
import sys
import types
import typing
from dataclasses import dataclass, field
from typing import get_type_hints

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

# Root package to scan.
ROOT_PACKAGE = "qdk"

# Modules to skip entirely (they are internal and not expected to have
# a clean public surface).
SKIP_MODULES: set[str] = {
    "qdk.telemetry",
    "qdk.telemetry_events",
}

# Only report violations for types whose __module__ starts with one of
# these prefixes.  This filters out false positives from standard-library
# and third-party packages whose internal types happen to have private
# __module__ paths (e.g. pathlib._local.Path, concurrent.futures._base.Future,
# qiskit._accelerate.target.QubitProperties).
OWNED_PREFIXES: tuple[str, ...] = ("qdk.",)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _is_private_name(name: str) -> bool:
    """Return True if *name* starts with underscore (Python private convention)."""
    return name.startswith("_") and not name.startswith("__")


def _module_has_private_segment(mod_name: str) -> bool:
    """Return True if any segment in a dotted module path is private.

    >>> _module_has_private_segment("qdk._native")
    True
    >>> _module_has_private_segment("qdk.qsharp")
    False
    """
    return any(_is_private_name(part) for part in mod_name.split("."))


def _type_fqn(tp: type) -> str:
    """Fully-qualified name of a type (best effort)."""
    mod = getattr(tp, "__module__", "") or ""
    qual = getattr(tp, "__qualname__", "") or getattr(tp, "__name__", str(tp))
    if mod:
        return f"{mod}.{qual}"
    return qual


def _extract_leaf_types(annotation) -> list:
    """Recursively unwrap generic aliases and return leaf types."""
    origin = getattr(annotation, "__origin__", None)
    args = getattr(annotation, "__args__", None)

    # typing special forms (Union, Optional, etc.)
    if origin is typing.Union or origin is types.UnionType:
        result = []
        for arg in args or ():
            result.extend(_extract_leaf_types(arg))
        return result

    if args:
        result = []
        if isinstance(origin, type):
            result.append(origin)
        for arg in args:
            result.extend(_extract_leaf_types(arg))
        return result

    if isinstance(annotation, type):
        return [annotation]

    # TypeVar — check bound and constraints
    if isinstance(annotation, typing.TypeVar):
        result = []
        if annotation.__bound__:
            result.extend(_extract_leaf_types(annotation.__bound__))
        for c in annotation.__constraints__:
            result.extend(_extract_leaf_types(c))
        return result

    # Forward reference (string annotation) – we can't resolve these
    # without the full module context, but check if the string looks private.
    if isinstance(annotation, (str, typing.ForwardRef)):
        return [annotation]

    return []


@dataclass
class Violation:
    """A single API-surface violation."""

    module: str
    public_symbol: str
    context: str  # e.g. "return type", "parameter 'foo'", "base class"
    private_ref: str  # the private type or module reference

    def __str__(self) -> str:
        return (
            f"{self.module}.{self.public_symbol}: "
            f"{self.context} references private {self.private_ref}"
        )


def _build_public_types(
    modules: list[tuple[str, types.ModuleType]],
) -> tuple[set[int], set[str]]:
    """Build the set of types that are publicly re-exported.

    Returns:
        A tuple of (public_type_ids, public_type_names) where:
        - public_type_ids is a set of ``id()`` values for type objects
          found in any public module's ``__all__``.
        - public_type_names is a set of unqualified names (e.g. "Config")
          for resolving forward-reference strings.
    """
    public_type_ids: set[int] = set()
    public_type_names: set[str] = set()

    for mod_name, mod in modules:
        all_symbols = getattr(mod, "__all__", None)
        if all_symbols is None:
            continue
        for sym_name in all_symbols:
            obj = getattr(mod, sym_name, None)
            if obj is None:
                continue
            if isinstance(obj, type):
                public_type_ids.add(id(obj))
                public_type_names.add(sym_name)

    return public_type_ids, public_type_names


def _check_annotation(
    annotation,
    module_name: str,
    symbol_name: str,
    context: str,
    violations: list[Violation],
    public_type_ids: set[int],
    public_type_names: set[str],
) -> None:
    """Check a single annotation for private-symbol leakage."""
    for leaf in _extract_leaf_types(annotation):
        if isinstance(leaf, (str, typing.ForwardRef)):
            ref_str = (
                leaf
                if isinstance(leaf, str)
                else (
                    leaf.__forward_arg__
                    if hasattr(leaf, "__forward_arg__")
                    else str(leaf)
                )
            )
            if _is_private_name(ref_str) or any(
                _is_private_name(p) for p in ref_str.split(".")
            ):
                # Check if the bare name matches a publicly-exported type
                bare_name = ref_str.rsplit(".", 1)[-1]
                if bare_name in public_type_names:
                    continue
                violations.append(Violation(module_name, symbol_name, context, ref_str))
        elif isinstance(leaf, type):
            # Skip types that are publicly re-exported
            if id(leaf) in public_type_ids:
                continue
            leaf_mod = getattr(leaf, "__module__", "") or ""
            leaf_name = getattr(leaf, "__qualname__", "") or getattr(
                leaf, "__name__", ""
            )
            # Only flag types owned by this project
            if not any(leaf_mod.startswith(p) for p in OWNED_PREFIXES):
                continue
            if _is_private_name(leaf_name) or _module_has_private_segment(leaf_mod):
                ref = _type_fqn(leaf)
                violations.append(Violation(module_name, symbol_name, context, ref))


def _check_callable(
    obj,
    module_name: str,
    symbol_name: str,
    violations: list[Violation],
    public_type_ids: set[int],
    public_type_names: set[str],
) -> None:
    """Inspect a function or method's annotations for private types."""
    try:
        hints = get_type_hints(obj)
    except Exception:
        # If we can't resolve hints (e.g. forward refs to unavailable types),
        # fall back to raw __annotations__.
        hints = getattr(obj, "__annotations__", {})

    for param_name, annotation in hints.items():
        if param_name == "return":
            context = "return type"
        else:
            context = f"parameter '{param_name}'"
        _check_annotation(
            annotation,
            module_name,
            symbol_name,
            context,
            violations,
            public_type_ids,
            public_type_names,
        )


def _check_class(
    cls: type,
    module_name: str,
    symbol_name: str,
    violations: list[Violation],
    public_type_ids: set[int],
    public_type_names: set[str],
) -> None:
    """Check a class's public methods' annotations for private types.

    Note: base classes are intentionally *not* checked.  A private base
    (mixin, ABC, protocol) is an implementation detail that users never
    reference directly, so it does not constitute actionable leakage.
    """
    # Check public methods — only those defined in qdk-owned modules.
    for attr_name in dir(cls):
        if attr_name.startswith("_") and not attr_name.startswith("__"):
            continue  # skip private methods
        try:
            attr = getattr(cls, attr_name)
        except Exception:
            continue
        if not callable(attr):
            continue
        if not (inspect.isfunction(attr) or inspect.ismethod(attr)):
            continue
        # Skip methods inherited from third-party / stdlib classes.
        defining_mod = getattr(attr, "__module__", None) or ""
        if (
            not any(defining_mod.startswith(p) for p in OWNED_PREFIXES)
            and defining_mod != ROOT_PACKAGE
        ):
            continue
        _check_callable(
            attr,
            module_name,
            f"{symbol_name}.{attr_name}",
            violations,
            public_type_ids,
            public_type_names,
        )


# ---------------------------------------------------------------------------
# Module discovery and scanning
# ---------------------------------------------------------------------------


def _iter_qdk_modules() -> list[tuple[str, types.ModuleType]]:
    """Import and yield all public qdk submodules."""
    root = importlib.import_module(ROOT_PACKAGE)
    result: list[tuple[str, types.ModuleType]] = [(ROOT_PACKAGE, root)]

    for importer, modname, ispkg in pkgutil.walk_packages(
        root.__path__, prefix=ROOT_PACKAGE + "."
    ):
        # Skip private modules entirely
        if any(_is_private_name(part) for part in modname.split(".")):
            continue
        if modname in SKIP_MODULES:
            continue
        try:
            mod = importlib.import_module(modname)
            result.append((modname, mod))
        except Exception as exc:
            print(f"WARNING: could not import {modname}: {exc}", file=sys.stderr)
    return result


def scan() -> list[Violation]:
    """Scan the qdk package and return all violations."""
    violations: list[Violation] = []

    modules = _iter_qdk_modules()
    public_type_ids, public_type_names = _build_public_types(modules)

    for mod_name, mod in modules:
        all_symbols = getattr(mod, "__all__", None)
        if all_symbols is None:
            continue  # only check modules that declare __all__

        for sym_name in all_symbols:
            obj = getattr(mod, sym_name, None)
            if obj is None:
                continue

            if isinstance(obj, type):
                _check_class(
                    obj,
                    mod_name,
                    sym_name,
                    violations,
                    public_type_ids,
                    public_type_names,
                )
            elif callable(obj):
                _check_callable(
                    obj,
                    mod_name,
                    sym_name,
                    violations,
                    public_type_ids,
                    public_type_names,
                )
            # For non-callable, non-class objects (e.g. TypeVar, constants),
            # we skip — they don't have annotations to check.

    return violations


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json", action="store_true", help="Output violations as JSON")
    args = parser.parse_args()

    violations = scan()

    if not violations:
        print("No private API leakage detected.", file=sys.stderr)
        return 0

    if args.json:
        print(
            json.dumps(
                [
                    {
                        "module": v.module,
                        "symbol": v.public_symbol,
                        "context": v.context,
                        "private_ref": v.private_ref,
                    }
                    for v in violations
                ],
                indent=2,
            )
        )
    else:
        # Compute unique private types for the summary line.
        unique_types = {v.private_ref for v in violations}

        print(
            f"\n{'='*70}\n"
            f" Private API leakage detected\n"
            f"   {len(unique_types)} private type(s), "
            f"{len(violations)} reference(s) across public API\n"
            f"{'='*70}\n",
            file=sys.stderr,
        )

        # Primary grouping: by private type (the actionable unit).
        by_type: dict[str, list[Violation]] = {}
        for v in violations:
            by_type.setdefault(v.private_ref, []).append(v)

        for priv_type, vs in sorted(by_type.items()):
            print(
                f"\n  {priv_type}  ({len(vs)} reference(s))",
                file=sys.stderr,
            )
            for v in vs:
                print(
                    f"    - {v.module}.{v.public_symbol}: {v.context}",
                    file=sys.stderr,
                )

        print(f"\n{'='*70}\n", file=sys.stderr)

    return 1


if __name__ == "__main__":
    sys.exit(main())
