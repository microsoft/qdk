# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations

from pathlib import Path
import time
from typing import Callable, Optional

from ..._qsharp import logical_counts
from ...estimator import LogicalCounts
from .._qre import Trace
from ..instruction_ids import CCX, MEAS_Z, RZ, T, READ_FROM_MEMORY, WRITE_TO_MEMORY
from ..property_keys import EVALUATION_TIME


def _bucketize_rotation_counts(
    rotation_count: int, rotation_depth: int
) -> list[tuple[int, int]]:
    """
    Returns a list of (count, depth) pairs representing the rotation layers in
    the trace.

    The following properties hold for the returned list `result`:
        - sum(depth for _, depth in result) == rotation_depth
        - sum(count * depth for count, depth in result) == rotation_count
        - count > 0 for each (count, _) in result
        - count <= qubit_count for each (count, _) in result holds by definition
          when rotation_count <= rotation_depth * qubit_count

    Args:
        rotation_count: Total number of rotations.
        rotation_depth: Total depth of the rotation layers.

    Returns:
        A list of (count, depth) pairs, where 'count' is the number of
        rotations in a layer and 'depth' is the depth of that layer.
    """
    if rotation_depth == 0:
        return []

    base = rotation_count // rotation_depth
    extra = rotation_count % rotation_depth

    result: list[tuple[int, int]] = []
    if extra > 0:
        result.append((base + 1, extra))
    if rotation_depth - extra > 0:
        result.append((base, rotation_depth - extra))
    return result


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

    if rotation_count != 0 and rotation_depth != 0:
        for count, depth in _bucketize_rotation_counts(rotation_count, rotation_depth):
            block = trace.add_block(repetitions=depth)
            for i in range(count):
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

    trace.set_property(EVALUATION_TIME, evaluation_time)
    return trace


def trace_from_entry_expr_cached(
    entry_expr: str | Callable | LogicalCounts, cache_path: Optional[Path]
) -> Trace:
    if cache_path and cache_path.exists():
        return Trace.from_json(cache_path.read_text())

    trace = trace_from_entry_expr(entry_expr)

    if cache_path:
        cache_path.parent.mkdir(parents=True, exist_ok=True)
        cache_path.write_text(trace.to_json())

    return trace
