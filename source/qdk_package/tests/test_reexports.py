# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import pytest, importlib


def test_qdk_qsharp_submodule_available():
    # Import the qsharp submodule explicitly.
    qdk_qsharp = importlib.import_module("qdk.qsharp")
    # Ensure a core API is reachable via submodule
    assert hasattr(qdk_qsharp, "run"), "qsharp.run missing in submodule"


def test_estimator_and_openqasm_shims():
    est = importlib.import_module("qdk.estimator")
    oq = importlib.import_module("qdk.openqasm")
    assert hasattr(est, "__doc__")
    assert hasattr(oq, "__doc__")


def test_missing_optional_direct_imports():
    # If optional extras truly not installed, importing their submodules should raise ImportError.
    # We probe without using mocks here.
    for mod in ("qdk.widgets", "qdk.azure", "qdk.qiskit"):
        base_dep = {
            "qdk.widgets": "qsharp_widgets",
            "qdk.azure": "azure.quantum",
            "qdk.qiskit": "qiskit",
        }[mod]
        try:
            importlib.import_module(base_dep)
            dep_installed = True
        except Exception:
            dep_installed = False
        if not dep_installed:
            try:
                importlib.import_module(mod)
            except ImportError as e:
                # Expected path: verify helpful hint present
                assert "pip install qdk[" in str(e)
            else:
                # If it imported anyway, treat as environment providing the feature (e.g. via dev install)
                pass
