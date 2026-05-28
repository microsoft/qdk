# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import pytest

try:
    # pylint: disable=unused-import
    import qiskit

    QISKIT_AVAILABLE = True
except ImportError:
    QISKIT_AVAILABLE = False

SKIP_REASON = "Qiskit is not available"


def ignore_on_failure(func):
    def wrapper(*args, **kwargs):
        try:
            func(*args, **kwargs)
        except Exception:
            pytest.skip("Test failed, skipping for now.")
        else:
            raise AssertionError("Test passed, remove decorator.")

    return wrapper
