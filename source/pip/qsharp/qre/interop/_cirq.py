# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations
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
    MeasurementGate,
    ResetChannel,
    GateOperation,
    ControlledOperation,
    ClassicallyControlledOperation,
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
)


def trace_from_cirq(circuit: cirq.Circuit) -> Trace:
    q_to_id = QidToTraceId(circuit.all_qubits())

    trace = Trace(len(circuit.all_qubits()))
    block = trace.root_block()

    context = cirq.DecompositionContext(
        qubit_manager=cirq.GreedyQubitManager("trace_from_cirq")
    )

    for moment in circuit:
        for op in moment.operations:
            handle_op(context, op, block, q_to_id)

    trace.compute_qubits = len(q_to_id)

    return trace


def handle_op(
    context: cirq.DecompositionContext,
    op: cirq.Operation,
    block: Block,
    q_to_id: QidToTraceId,
) -> None:
    if isinstance(op, GateOperation):
        gate = op.gate

        if hasattr(gate, "_to_trace"):
            gate._to_trace(op, block, q_to_id)  # type: ignore
        elif hasattr(gate, "_decompose_with_context_"):
            for sub_op in gate._decompose_with_context_(op.qubits, context):  # type: ignore
                handle_op(context, sub_op, block, q_to_id)
        elif hasattr(gate, "_decompose_"):
            # TODO: Cache similar calls?
            # Decompose the gate and handle the resulting operations recursively
            for sub_op in gate._decompose_(op.qubits):  # type: ignore
                handle_op(context, sub_op, block, q_to_id)
        else:
            raise NotImplementedError(
                f"Unsupported gate operation: {gate} {type(gate)}"
            )
    elif isinstance(op, ControlledOperation):
        # TODO: check if there is an advantage to check whether this contains a controlled gate first?
        for sub_op in op._decompose_with_context_(context):  # type: ignore
            handle_op(context, sub_op, block, q_to_id)
    elif isinstance(op, ClassicallyControlledOperation):
        # TODO: Take into account the classical control probability
        handle_op(context, op.without_classical_controls(), block, q_to_id)
    elif isinstance(op, list):
        for sub_op in op:
            handle_op(context, sub_op, block, q_to_id)
    else:
        raise NotImplementedError(f"Unsupported operation: {op} {type(op)}")


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


def h_pow_gate_to_trace(
    self, op: cirq.Operation, block: Block, q_to_id: QidToTraceId
) -> None:
    if abs(self.exponent) != 1:
        raise NotImplementedError(
            f"Unsupported HPowGate with exponent {self.exponent}."
        )

    block.add_operation(H, [q_to_id[op.qubits[0]]])


def x_pow_gate_to_trace(
    self, op: cirq.Operation, block: Block, q_to_id: QidToTraceId
) -> None:
    if abs(self.exponent - 1) <= 1e-8 or abs(self.exponent + 1) <= 1e-8:
        block.add_operation(PAULI_X, [q_to_id[op.qubits[0]]])
    elif abs(self.exponent - 0.5) <= 1e-8:
        block.add_operation(SQRT_X, [q_to_id[op.qubits[0]]])
    elif abs(self.exponent + 0.5) <= 1e-8:
        block.add_operation(SQRT_X_DAG, [q_to_id[op.qubits[0]]])
    elif abs(self.exponent - 0.25) <= 1e-8:
        block.add_operation(SQRT_SQRT_X, [q_to_id[op.qubits[0]]])
    elif abs(self.exponent + 0.25) <= 1e-8:
        block.add_operation(SQRT_SQRT_X_DAG, [q_to_id[op.qubits[0]]])
    else:
        block.add_operation(RX, [q_to_id[op.qubits[0]]], [self.exponent * pi])


def y_pow_gate_to_trace(
    self, op: cirq.Operation, block: Block, q_to_id: QidToTraceId
) -> None:
    if abs(self.exponent - 1) <= 1e-8 or abs(self.exponent + 1) <= 1e-8:
        block.add_operation(PAULI_Y, [q_to_id[op.qubits[0]]])
    elif abs(self.exponent - 0.5) <= 1e-8:
        block.add_operation(SQRT_Y, [q_to_id[op.qubits[0]]])
    elif abs(self.exponent + 0.5) <= 1e-8:
        block.add_operation(SQRT_Y_DAG, [q_to_id[op.qubits[0]]])
    elif abs(self.exponent - 0.25) <= 1e-8:
        block.add_operation(SQRT_SQRT_Y, [q_to_id[op.qubits[0]]])
    elif abs(self.exponent + 0.25) <= 1e-8:
        block.add_operation(SQRT_SQRT_Y_DAG, [q_to_id[op.qubits[0]]])
    else:
        block.add_operation(RY, [q_to_id[op.qubits[0]]], [self.exponent * pi])


def z_pow_gate_to_trace(
    self, op: cirq.Operation, block: Block, q_to_id: QidToTraceId
) -> None:
    if abs(self.exponent - 1) <= 1e-8 or abs(self.exponent + 1) <= 1e-8:
        block.add_operation(PAULI_Z, [q_to_id[op.qubits[0]]])
    elif abs(self.exponent - 0.5) <= 1e-8:
        block.add_operation(S, [q_to_id[op.qubits[0]]])
    elif abs(self.exponent + 0.5) <= 1e-8:
        block.add_operation(S_DAG, [q_to_id[op.qubits[0]]])
    elif abs(self.exponent - 0.25) <= 1e-8:
        block.add_operation(T, [q_to_id[op.qubits[0]]])
    elif abs(self.exponent + 0.25) <= 1e-8:
        block.add_operation(T_DAG, [q_to_id[op.qubits[0]]])
    else:
        block.add_operation(RZ, [q_to_id[op.qubits[0]]], [self.exponent * pi])


def cx_pow_gate_to_trace(
    self, op: cirq.Operation, block: Block, q_to_id: QidToTraceId
) -> None:
    if abs(self.exponent) != 1:
        raise NotImplementedError(
            f"Unsupported CXPowGate with exponent {self.exponent}."
        )

    block.add_operation(CX, [q_to_id[op.qubits[0]], q_to_id[op.qubits[1]]])


def cz_pow_gate_to_trace(
    self, op: cirq.Operation, block: Block, q_to_id: QidToTraceId
) -> None:
    if abs(self.exponent - 1) <= 1e-8 or abs(self.exponent + 1) <= 1e-8:
        block.add_operation(CZ, [q_to_id[op.qubits[0]], q_to_id[op.qubits[1]]])
    elif abs(self.exponent - 0.5) <= 1e-8:
        # controlled S gate
        c, t = q_to_id[op.qubits[0]], q_to_id[op.qubits[1]]
        block.add_operation(T, [c])
        block.add_operation(T, [t])
        block.add_operation(CZ, [c, t])
        block.add_operation(T_DAG, [t])
        block.add_operation(CZ, [c, t])
    elif abs(self.exponent + 0.5) <= 1e-8:
        # controlled S† gate
        c, t = q_to_id[op.qubits[0]], q_to_id[op.qubits[1]]
        block.add_operation(T_DAG, [c])
        block.add_operation(T_DAG, [t])
        block.add_operation(CZ, [c, t])
        block.add_operation(T, [t])
        block.add_operation(CZ, [c, t])
    else:
        # Half the exponent and translate into radians
        rads = self.exponent / 2 * pi
        c, t = q_to_id[op.qubits[0]], q_to_id[op.qubits[1]]
        block.add_operation(RZ, [c], [rads])
        block.add_operation(RZ, [t], [rads])
        block.add_operation(CZ, [c, t])
        block.add_operation(RZ, [t], [-rads])
        block.add_operation(CZ, [c, t])

    block.add_operation(CZ, [q_to_id[op.qubits[0]], q_to_id[op.qubits[1]]])


def measurement_gate_to_trace(
    self, op: cirq.Operation, block: Block, q_to_id: QidToTraceId
) -> None:

    for q in op.qubits:
        block.add_operation(MEAS_Z, [q_to_id[q]])


def reset_channel_to_trace(
    self, op: cirq.Operation, block: Block, q_to_id: QidToTraceId
) -> None:
    pass


HPowGate._to_trace = h_pow_gate_to_trace
XPowGate._to_trace = x_pow_gate_to_trace
YPowGate._to_trace = y_pow_gate_to_trace
ZPowGate._to_trace = z_pow_gate_to_trace
CXPowGate._to_trace = cx_pow_gate_to_trace
CZPowGate._to_trace = cz_pow_gate_to_trace
MeasurementGate._to_trace = measurement_gate_to_trace
ResetChannel._to_trace = reset_channel_to_trace
