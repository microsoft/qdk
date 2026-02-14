# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations

import time
from typing import Callable

from ..._qsharp import logical_counts
from ...estimator import LogicalCounts
from .._qre import Trace
from ..instruction_ids import CCX, MEAS_Z, RZ, T, READ_FROM_MEMORY, WRITE_TO_MEMORY


def trace_from_entry_expr(entry_expr: str | Callable | LogicalCounts) -> Trace:

    start = time.time_ns()
    counts = (
        logical_counts(entry_expr)
        if not isinstance(entry_expr, LogicalCounts)
        else entry_expr
    )
    evaluation_time = time.time_ns() - start

    ccx_count = counts.get("cczCount", 0) + counts.get("ccixCount", 0)

    # Q# logical counts report total number of qubits (compute + memory)
    num_qubits = counts.get("numQubits", 0)
    # Compute qubits may be reported separately
    compute_qubits = counts.get("numComputeQubits", num_qubits)
    memory_qubits = num_qubits - compute_qubits

    trace = Trace(compute_qubits)

    rotation_count = counts.get("rotationCount", 0)
    rotation_depth = counts.get("rotationDepth", rotation_count)

    if rotation_count != 0:
        if rotation_depth > 1:
            rotations_per_layer = rotation_count // (rotation_depth - 1)
        else:
            rotations_per_layer = 0

        last_layer = rotation_count - (rotations_per_layer * (rotation_depth - 1))

        if rotations_per_layer != 0:
            block = trace.add_block(repetitions=rotation_depth - 1)
            for i in range(rotations_per_layer):
                block.add_operation(RZ, [i])
        block = trace.add_block()
        for i in range(last_layer):
            block.add_operation(RZ, [i])

    if t_count := counts.get("tCount", 0):
        block = trace.add_block(repetitions=t_count)
        block.add_operation(T, [0])

    if ccx_count:
        block = trace.add_block(repetitions=ccx_count)
        block.add_operation(CCX, [0, 1, 2])

    if meas_count := counts.get("measurementCount", 0):
        block = trace.add_block(repetitions=meas_count)
        block.add_operation(MEAS_Z, [0])

    if memory_qubits != 0:
        trace.set_memory_qubits(memory_qubits)

        if rfm_count := counts.get("readFromMemoryCount", 0):
            block = trace.add_block(repetitions=rfm_count)
            block.add_operation(READ_FROM_MEMORY, [0, compute_qubits])

        if wtm_count := counts.get("writeToMemoryCount", 0):
            block = trace.add_block(repetitions=wtm_count)
            block.add_operation(WRITE_TO_MEMORY, [0, compute_qubits])

    trace.set_property("evaluation_time", evaluation_time)
    return trace
