# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Tests for the re-export shims that wrap optional third-party packages.

Only ``qdk.widgets`` (wraps ``qsharp_widgets``) and ``qdk.azure`` (wraps
``azure.quantum``) are re-export shims. We mock the upstream packages and
verify that the shims surface the expected attributes.
"""

import importlib
import pytest

from mocks import mock_widgets, mock_azure, cleanup_modules


MOCK_EXTRAS = {
    "widgets": {
        "mock": mock_widgets,
        "module": "qdk.widgets",
        "post_assert": lambda mod: hasattr(mod, "__doc__"),
    },
    "azure": {
        "mock": mock_azure,
        "module": "qdk.azure",
        "post_assert": lambda mod: all(
            hasattr(mod, name) for name in ("target", "argument_types", "job")
        ),
    },
}


@pytest.mark.parametrize("name,spec", MOCK_EXTRAS.items())
def test_reexport_shim_with_mock(name, spec):
    created = spec["mock"]()
    try:
        imported = importlib.import_module(spec["module"])
        assert spec["post_assert"](imported)
    finally:
        cleanup_modules(created)
