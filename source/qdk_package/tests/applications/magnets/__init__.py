# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Unit tests for the magnets library."""

try:
    # pylint: disable=unused-import
    # flake8: noqa E401
    import cirq

    CIRQ_AVAILABLE = True
except ImportError:
    CIRQ_AVAILABLE = False

SKIP_REASON = "cirq is not available"
