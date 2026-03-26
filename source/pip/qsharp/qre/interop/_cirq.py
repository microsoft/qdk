# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations
from dataclasses import dataclass
from math import pi
from typing import Iterable

import cirq
from cirq import (
    HPowGate,
    XPowGate,
    YPowGate,
    ZPowGate,
    CXPowGate,
    CZPowGate,
    CCXPowGate,
    CCZPowGate,
    MeasurementGate,
    ResetChannel,
    GateOperation,
    ControlledOperation,
    ClassicallyControlledOperation,
    PhaseGradientGate,
)
from qsharp.qre import Trace, Block
from qsharp.qre.instruction_ids import (
    H,
    PAULI_X,
    PAULI_Y,
    PAULI_Z,
    SQRT_X,
    SQRT_X_DAG,
    SQRT_SQRT_X,
    SQRT_SQRT_X_DAG,
    SQRT_Y,
    SQRT_Y_DAG,
    SQRT_SQRT_Y,
    SQRT_SQRT_Y_DAG,
    S,
    S_DAG,
    T,
    T_DAG,
    CX,
    CZ,
    RX,
    RY,
    RZ,
    MEAS_Z,
    CCX,
    CCZ,
)


def trace_from_cirq(circuit: cirq.Circuit) -> Trace:
    q_to_id = QidToTraceId(circuit.all_qubits())

    trace = Trace(len(circuit.all_qubits()))

    context = cirq.DecompositionContext(
        qubit_manager=cirq.GreedyQubitManager("trace_from_cirq")
    )

    generation_context = _Context(trace)

    for moment in circuit:
        for op in moment.operations:
            handle_op(context, op, generation_context, q_to_id)

    trace.compute_qubits = len(q_to_id)

    return trace


def handle_op(
    context: cirq.DecompositionContext,
    op: (
        cirq.Operation
        | tuple[int, list[cirq.Qid] | cirq.Qid]
        | tuple[int, list[cirq.Qid] | cirq.Qid, list[float] | float]
        | PushBlock
        | PopBlock
    ),
    generation_context: _Context,
    q_to_id: QidToTraceId,
) -> None:
    if isinstance(op, tuple):
        if len(op) == 2:
            id, qubits = op
            params = None
        elif len(op) == 3:
            id, qubits, params = op

        qs = [
            q_to_id[q] for q in ([qubits] if isinstance(qubits, cirq.Qid) else qubits)
        ]

        if params is None:
            generation_context.block.add_operation(id, qs)
        else:
            generation_context.block.add_operation(
                id, qs, params if isinstance(params, list) else [params]
            )
    elif isinstance(op, PushBlock):
        generation_context.push_block(op.repetitions)
    elif isinstance(op, PopBlock):
        generation_context.pop_block()
    elif isinstance(op, GateOperation):
        gate = op.gate

        if hasattr(gate, "_to_trace"):
            for sub_op in gate._to_trace(context, op):  # type: ignore
                handle_op(context, sub_op, generation_context, q_to_id)
        elif hasattr(gate, "_decompose_with_context_"):
            for sub_op in gate._decompose_with_context_(op.qubits, context):  # type: ignore
                handle_op(context, sub_op, generation_context, q_to_id)
        elif hasattr(gate, "_decompose_"):
            # decompose the gate and handle the resulting operations recursively
            for sub_op in gate._decompose_(op.qubits):  # type: ignore
                handle_op(context, sub_op, generation_context, q_to_id)
        else:
            raise NotImplementedError(
                f"Unsupported gate operation: {gate} {type(gate)}"
            )
    elif isinstance(op, ControlledOperation):
        # TODO: check if there is an advantage to check whether this contains a controlled gate first?
        for sub_op in op._decompose_with_context_(context):  # type: ignore
            handle_op(context, sub_op, generation_context, q_to_id)
    elif isinstance(op, ClassicallyControlledOperation):
        # TODO: Take into account the classical control probability
        handle_op(context, op.without_classical_controls(), generation_context, q_to_id)
    elif isinstance(op, list):
        for sub_op in op:
            handle_op(context, sub_op, generation_context, q_to_id)

    else:
        raise NotImplementedError(f"Unsupported operation: {op} {type(op)}")


class _Context:
    def __init__(self, trace: Trace):
        self._trace = trace
        self._blocks = [trace.root_block()]

    def push_block(self, repetitions: int):
        block = self.block.add_block(repetitions)
        self._blocks.append(block)

    def pop_block(self):
        self._blocks.pop()

    @property
    def block(self) -> Block:
        return self._blocks[-1]


@dataclass(frozen=True, slots=True)
class PushBlock:
    repetitions: int


@dataclass(frozen=True, slots=True)
class PopBlock: ...


class QidToTraceId(dict):
    def __init__(self, init: Iterable[cirq.Qid]):
        super().__init__({q: i for i, q in enumerate(init)})

    def __getitem__(self, key: cirq.Qid) -> int:
        """
        If the key is not present, add it to the mapping with the next available id.
        """

        if key not in self:
            self[key] = len(self)
        return super().__getitem__(key)


def h_pow_gate_to_trace(self, context: cirq.DecompositionContext, op: cirq.Operation):
    if abs(self.exponent) == 1:
        yield (H, [op.qubits[0]])
    else:
        yield from op._decompose_with_context_(context)  # type: ignore


def x_pow_gate_to_trace(self, context: cirq.DecompositionContext, op: cirq.Operation):
    if abs(self.exponent - 1) <= 1e-8 or abs(self.exponent + 1) <= 1e-8:
        yield (PAULI_X, [op.qubits[0]])
    elif abs(self.exponent - 0.5) <= 1e-8:
        yield (SQRT_X, [op.qubits[0]])
    elif abs(self.exponent + 0.5) <= 1e-8:
        yield (SQRT_X_DAG, [op.qubits[0]])
    elif abs(self.exponent - 0.25) <= 1e-8:
        yield (SQRT_SQRT_X, [op.qubits[0]])
    elif abs(self.exponent + 0.25) <= 1e-8:
        yield (SQRT_SQRT_X_DAG, [op.qubits[0]])
    else:
        yield (RX, [op.qubits[0]], self.exponent * pi)


def y_pow_gate_to_trace(self, context: cirq.DecompositionContext, op: cirq.Operation):
    if abs(self.exponent - 1) <= 1e-8 or abs(self.exponent + 1) <= 1e-8:
        yield (PAULI_Y, [op.qubits[0]])
    elif abs(self.exponent - 0.5) <= 1e-8:
        yield (SQRT_Y, [op.qubits[0]])
    elif abs(self.exponent + 0.5) <= 1e-8:
        yield (SQRT_Y_DAG, [op.qubits[0]])
    elif abs(self.exponent - 0.25) <= 1e-8:
        yield (SQRT_SQRT_Y, [op.qubits[0]])
    elif abs(self.exponent + 0.25) <= 1e-8:
        yield (SQRT_SQRT_Y_DAG, [op.qubits[0]])
    else:
        yield (RY, [op.qubits[0]], self.exponent * pi)


def z_pow_gate_to_trace(self, context: cirq.DecompositionContext, op: cirq.Operation):
    if abs(self.exponent - 1) <= 1e-8 or abs(self.exponent + 1) <= 1e-8:
        yield (PAULI_Z, [op.qubits[0]])
    elif abs(self.exponent - 0.5) <= 1e-8:
        yield (S, [op.qubits[0]])
    elif abs(self.exponent + 0.5) <= 1e-8:
        yield (S_DAG, [op.qubits[0]])
    elif abs(self.exponent - 0.25) <= 1e-8:
        yield (T, [op.qubits[0]])
    elif abs(self.exponent + 0.25) <= 1e-8:
        yield (T_DAG, [op.qubits[0]])
    else:
        yield (RZ, [op.qubits[0]], self.exponent * pi)


def cx_pow_gate_to_trace(self, context: cirq.DecompositionContext, op: cirq.Operation):
    if abs(self.exponent) != 1:
        raise NotImplementedError(
            f"Unsupported CXPowGate with exponent {self.exponent}."
        )

    yield (CX, [op.qubits[0], op.qubits[1]])


def cz_pow_gate_to_trace(self, context: cirq.DecompositionContext, op: cirq.Operation):
    if abs(self.exponent - 1) <= 1e-8 or abs(self.exponent + 1) <= 1e-8:
        yield (CZ, [op.qubits[0], op.qubits[1]])
    elif abs(self.exponent - 0.5) <= 1e-8:
        # controlled S gate
        c, t = op.qubits[0], op.qubits[1]
        yield (T, [c])
        yield (T, [t])
        yield (CZ, [c, t])
        yield (T_DAG, [t])
        yield (CZ, [c, t])
    elif abs(self.exponent + 0.5) <= 1e-8:
        # controlled S† gate
        c, t = op.qubits[0], op.qubits[1]
        yield (T_DAG, [c])
        yield (T_DAG, [t])
        yield (CZ, [c, t])
        yield (T, [t])
        yield (CZ, [c, t])
    else:
        # Half the exponent and translate into radians
        rads = self.exponent / 2 * pi
        c, t = op.qubits[0], op.qubits[1]
        yield (RZ, [c], [rads])
        yield (RZ, [t], [rads])
        yield (CZ, [c, t])
        yield (RZ, [t], [-rads])
        yield (CZ, [c, t])


def ccx_pow_gate_to_trace(self, context: cirq.DecompositionContext, op: cirq.Operation):
    if abs(self.exponent) == 1:
        yield (CCX, [op.qubits[0], op.qubits[1], op.qubits[2]])
    else:
        yield from op._decompose_with_context_(context)  # type: ignore


def ccz_pow_gate_to_trace(self, context: cirq.DecompositionContext, op: cirq.Operation):
    if abs(self.exponent) == 1:
        yield (CCZ, [op.qubits[0], op.qubits[1], op.qubits[2]])
    else:
        yield from op._decompose_with_context_(context)  # type: ignore


def measurement_gate_to_trace(
    self, context: cirq.DecompositionContext, op: cirq.Operation
):
    for q in op.qubits:
        yield (MEAS_Z, [q])


def reset_channel_to_trace(
    self, context: cirq.DecompositionContext, op: cirq.Operation
):
    return
    yield


HPowGate._to_trace = h_pow_gate_to_trace
XPowGate._to_trace = x_pow_gate_to_trace
YPowGate._to_trace = y_pow_gate_to_trace
ZPowGate._to_trace = z_pow_gate_to_trace
CXPowGate._to_trace = cx_pow_gate_to_trace
CZPowGate._to_trace = cz_pow_gate_to_trace
CCXPowGate._to_trace = ccx_pow_gate_to_trace
CCZPowGate._to_trace = ccz_pow_gate_to_trace
MeasurementGate._to_trace = measurement_gate_to_trace
ResetChannel._to_trace = reset_channel_to_trace

# Decomposition overrides


def phase_gradient_decompose(self, qubits):
    """
    Overrides implementation of PhaseGradientGate._decompose_ to skip rotations
    with very small angles.  In particular the original implementation may lead
    to FP overflows for large values of i.
    """

    for i, q in enumerate(qubits):
        exp = self.exponent / 2**i
        if exp < 1e-16:
            break
        yield cirq.Z(q) ** exp


PhaseGradientGate._decompose_ = phase_gradient_decompose
