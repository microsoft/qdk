# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Tests for qdk re-export shims.

Only ``qdk.widgets`` and ``qdk.azure`` are re-export shims wrapping third-party
packages.  All other qdk submodules (estimator, openqasm, qiskit, cirq, qre,
etc.) now own their code directly and are covered by functional tests elsewhere.
"""

import importlib
import pytest


# ---- Friendly error messages when optional deps are missing ----

_REEXPORT_SHIMS = {
    "qdk.widgets": {"dep": "qsharp_widgets", "hint": "pip install qdk[jupyter]"},
    "qdk.azure": {"dep": "azure.quantum", "hint": "pip install qdk[azure]"},
}


@pytest.mark.parametrize("mod,spec", _REEXPORT_SHIMS.items())
def test_missing_optional_gives_helpful_error(mod, spec):
    """When the upstream dep is absent, importing the shim should raise
    ImportError containing a pip-install hint."""
    try:
        importlib.import_module(spec["dep"])
        pytest.skip(f"{spec['dep']} is installed; cannot test missing-dep path")
    except ImportError:
        pass

    with pytest.raises(ImportError, match=spec["hint"]):
        importlib.import_module(mod)
