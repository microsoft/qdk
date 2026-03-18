# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

try:
    import cirq  # noqa: F401

    CIRQ_AVAILABLE = True
except ImportError:
    CIRQ_AVAILABLE = False

SKIP_REASON = "Cirq is not available"
