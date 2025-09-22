# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import pytest, importlib

from mocks import (
    mock_widgets,
    mock_azure,
    mock_qiskit,
    cleanup_modules,
)


# Standard contract description for each extra we test.
EXTRAS = {
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
    "qiskit": {
        "mock": mock_qiskit,
        "module": "qdk.qiskit",
        "post_assert": lambda mod: hasattr(mod, "transpile"),
    },
}


@pytest.mark.parametrize("name,spec", EXTRAS.items())
def test_direct_import_with_mock(name, spec):
    created = spec["mock"]()
    try:
        imported = importlib.import_module(spec["module"])
        assert spec["post_assert"](imported)
    finally:
        cleanup_modules(created)
