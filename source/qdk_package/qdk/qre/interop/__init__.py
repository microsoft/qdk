# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from ._qir import trace_from_qir
from ._qsharp import trace_from_entry_expr, trace_from_entry_expr_cached

try:
    from ._cirq import (
        PeakUsageGreedyQubitManager,
        PopBlock,
        PushBlock,
        QubitType,
        ReadFromMemoryGate,
        TypedQubit,
        WriteToMemoryGate,
        assert_qubits_type,
        read_from_memory,
        trace_from_cirq,
        write_to_memory,
    )
except ImportError:
    _CIRQ_INSTALL_MSG = (
        "This function requires the 'cirq' extra. "
        "Install it with: pip install qdk[qre,cirq]"
    )

    class _CirqNotInstalled:
        """Placeholder that raises a helpful error when cirq is not installed."""

        def __init__(self, *args, **kwargs):
            raise ImportError(_CIRQ_INSTALL_MSG)

    def _cirq_not_installed_func(*args, **kwargs):
        """Placeholder that raises a helpful error when cirq is not installed."""
        raise ImportError(_CIRQ_INSTALL_MSG)

    PeakUsageGreedyQubitManager = _CirqNotInstalled
    PopBlock = _CirqNotInstalled
    PushBlock = _CirqNotInstalled
    QubitType = _CirqNotInstalled
    ReadFromMemoryGate = _CirqNotInstalled
    TypedQubit = _CirqNotInstalled
    WriteToMemoryGate = _CirqNotInstalled
    assert_qubits_type = _cirq_not_installed_func
    read_from_memory = _cirq_not_installed_func
    trace_from_cirq = _cirq_not_installed_func
    write_to_memory = _cirq_not_installed_func

__all__ = [
    "PeakUsageGreedyQubitManager",
    "PopBlock",
    "PushBlock",
    "QubitType",
    "ReadFromMemoryGate",
    "TypedQubit",
    "WriteToMemoryGate",
    "assert_qubits_type",
    "read_from_memory",
    "trace_from_cirq",
    "trace_from_entry_expr",
    "trace_from_entry_expr_cached",
    "trace_from_qir",
    "write_to_memory",
]
