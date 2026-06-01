#!/usr/bin/env python3

# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Lint: detect private-symbol leakage in the qdk public API surface.

For every symbol listed in a module's ``__all__``, this script inspects
function / method type annotations and class base classes.  If any
annotation or base references an underscore-prefixed name (e.g.
``_native.Circuit``) or a type whose ``__module__`` contains an
underscore-prefixed segment (e.g. ``qdk._types.Config``), a violation
is reported.

Exit code 0  – no violations found.
Exit code 1  – one or more violations found (details printed to stderr).

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
    "qdk._native",
    "qdk._types",
    "qdk._context",
    "qdk._interpreter",
    "qdk._ipython",
    "qdk._fs",
    "qdk._http",
    "qdk._adaptive_bytecode",
    "qdk._adaptive_pass",
    "qdk._device",
    "qdk.telemetry",
    "qdk.telemetry_events",
}

# Only report violations for types whose __module__ starts with one of
# these prefixes.  This filters out false positives from standard-library
# and third-party packages whose internal types happen to have private
# __module__ paths (e.g. pathlib._local.Path, concurrent.futures._base.Future,
# qiskit._accelerate.target.QubitProperties).
OWNED_PREFIXES: tuple[str, ...] = ("qdk.",)

# Symbols that are themselves known-OK despite having a private-looking
# module path (e.g. the Rust extension types that we *intend* to re-export).
# Format: frozenset of fully qualified "<module>.<qualname>" strings.
KNOWN_EXCEPTIONS: frozenset[str] = frozenset()


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


def _check_annotation(
    annotation,
    module_name: str,
    symbol_name: str,
    context: str,
    violations: list[Violation],
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
                fqn = f"{module_name}.{symbol_name}"
                if fqn not in KNOWN_EXCEPTIONS:
                    violations.append(
                        Violation(module_name, symbol_name, context, ref_str)
                    )
        elif isinstance(leaf, type):
            leaf_mod = getattr(leaf, "__module__", "") or ""
            leaf_name = getattr(leaf, "__qualname__", "") or getattr(
                leaf, "__name__", ""
            )
            # Only flag types owned by this project
            if not any(leaf_mod.startswith(p) for p in OWNED_PREFIXES):
                continue
            if _is_private_name(leaf_name) or _module_has_private_segment(leaf_mod):
                ref = _type_fqn(leaf)
                fqn = f"{module_name}.{symbol_name}"
                if fqn not in KNOWN_EXCEPTIONS:
                    violations.append(Violation(module_name, symbol_name, context, ref))


def _check_callable(
    obj,
    module_name: str,
    symbol_name: str,
    violations: list[Violation],
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
        _check_annotation(annotation, module_name, symbol_name, context, violations)


def _check_class(
    cls: type,
    module_name: str,
    symbol_name: str,
    violations: list[Violation],
) -> None:
    """Check a class's bases, and its public methods' annotations."""
    # Check base classes
    for base in cls.__mro__[1:]:  # skip the class itself
        if base is object:
            continue
        base_mod = getattr(base, "__module__", "") or ""
        base_name = getattr(base, "__qualname__", "") or ""
        # Only flag types owned by this project
        if not any(base_mod.startswith(p) for p in OWNED_PREFIXES):
            continue
        if _is_private_name(base_name) or _module_has_private_segment(base_mod):
            ref = _type_fqn(base)
            violations.append(Violation(module_name, symbol_name, "base class", ref))

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

    for mod_name, mod in _iter_qdk_modules():
        all_symbols = getattr(mod, "__all__", None)
        if all_symbols is None:
            continue  # only check modules that declare __all__

        for sym_name in all_symbols:
            obj = getattr(mod, sym_name, None)
            if obj is None:
                continue

            if isinstance(obj, type):
                _check_class(obj, mod_name, sym_name, violations)
            elif callable(obj):
                _check_callable(obj, mod_name, sym_name, violations)
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
        print(
            f"\n{'='*70}\n"
            f" Private API leakage: {len(violations)} violation(s) found\n"
            f"{'='*70}\n",
            file=sys.stderr,
        )
        # Group by module for readability
        by_module: dict[str, list[Violation]] = {}
        for v in violations:
            by_module.setdefault(v.module, []).append(v)

        for mod, vs in sorted(by_module.items()):
            print(f"\n  {mod}:", file=sys.stderr)
            for v in vs:
                print(
                    f"    - {v.public_symbol}: {v.context} -> {v.private_ref}",
                    file=sys.stderr,
                )

        print(f"\n{'='*70}\n", file=sys.stderr)

    return 1


if __name__ == "__main__":
    sys.exit(main())
