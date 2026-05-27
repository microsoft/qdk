# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Tests for the ``qsharp`` compatibility shim.

The ``qsharp`` package is a thin deprecation wrapper that re-exports the
public API from ``qdk``.  These tests verify that the expected symbols and
submodules are accessible via ``import qsharp``.
"""

import importlib
import warnings

import pytest

# ---- Symbols that must be directly accessible on ``qsharp`` ----

_EXPECTED_SYMBOLS = [
    # functions
    "init",
    "eval",
    "run",
    "compile",
    "circuit",
    "estimate",
    "estimate_custom",
    "logical_counts",
    "set_quantum_seed",
    "set_classical_seed",
    "dump_machine",
    "dump_circuit",
    # types / classes
    "Result",
    "Pauli",
    "QSharpError",
    "TargetProfile",
    "StateDump",
    "ShotResult",
    "PauliNoise",
    "DepolarizingNoise",
    "BitFlipNoise",
    "PhaseFlipNoise",
    "CircuitGenerationMethod",
]

# Submodules that must be importable *and* accessible as attributes
# on the ``qsharp`` module after a bare ``import qsharp``.
_EXPECTED_SUBMODULES = [
    "code",
    "estimator",
]


@pytest.fixture()
def qsharp_module():
    """Import the ``qsharp`` shim while suppressing the deprecation warning."""
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", DeprecationWarning)
        mod = importlib.import_module("qsharp")
    return mod


def test_deprecation_warning_on_import():
    """Importing ``qsharp`` must emit a DeprecationWarning."""
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        importlib.reload(importlib.import_module("qsharp"))

    deprecations = [w for w in caught if issubclass(w.category, DeprecationWarning)]
    assert any("qsharp" in str(w.message) for w in deprecations)


@pytest.mark.parametrize("name", _EXPECTED_SYMBOLS)
def test_symbol_accessible(qsharp_module, name):
    """Every expected symbol must be an attribute of ``qsharp``."""
    assert hasattr(qsharp_module, name), f"qsharp.{name} is missing"


@pytest.mark.parametrize("name", _EXPECTED_SUBMODULES)
def test_submodule_accessible_as_attribute(qsharp_module, name):
    """Submodules like ``code`` must be reachable via ``qsharp.<name>``
    without a separate ``from qsharp import <name>``."""
    attr = getattr(qsharp_module, name, None)
    assert attr is not None, f"qsharp.{name} is not accessible as an attribute"


@pytest.mark.parametrize("name", _EXPECTED_SUBMODULES)
def test_submodule_importable(name):
    """``from qsharp import <submodule>`` must work."""
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", DeprecationWarning)
        mod = importlib.import_module(f"qsharp.{name}")
    assert mod is not None
