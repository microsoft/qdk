# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations

import random
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
    ClassicallyControlledOperation,
    PhaseGradientGate,
    SwapPowGate,
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
    SWAP,
)

_TOLERANCE = 1e-8


def _approx_eq(a: float, b: float) -> bool:
    """Check whether two floats are approximately equal."""
    return abs(a - b) <= _TOLERANCE


def trace_from_cirq(
    circuit: cirq.Circuit, *, classical_control_probability: float = 0.5
) -> Trace:
    """Convert a Cirq circuit into a resource estimation Trace.

    Iterates through all moments and operations in the circuit, converting
    each gate into trace operations. Gates with a ``_to_trace`` method are
    converted directly; others are recursively decomposed via Cirq's
    ``_decompose_with_context_`` or ``_decompose_`` protocols.

    Args:
        circuit: The Cirq circuit to convert.
        classical_control_probability: Probability that a classically
            controlled operation is included in the trace. Defaults to 0.5.

    Returns:
        A Trace representing the resource profile of the circuit.
    """

    context = _Context(circuit, classical_control_probability)

    for moment in circuit:
        for op in moment.operations:
            context.handle_op(op)

    return context.trace


class _Context:
    """Tracks the current trace and block nesting during trace generation.

    Maintains a stack of blocks so that ``PushBlock`` and ``PopBlock``
    operations can create nested repeated sections in the trace.
    """

    def __init__(self, circuit: cirq.Circuit, classical_control_probability: float):
        self._trace = Trace(len(circuit.all_qubits()))
        self._classical_control_probability = classical_control_probability
        self._blocks = [self._trace.root_block()]
        self._q_to_id = _QidToTraceId(circuit.all_qubits())
        self._decomp_context = cirq.DecompositionContext(
            qubit_manager=cirq.GreedyQubitManager("trace_from_cirq")
        )

    def push_block(self, repetitions: int):
        block = self.block.add_block(repetitions)
        self._blocks.append(block)

    def pop_block(self):
        self._blocks.pop()

    @property
    def trace(self) -> Trace:
        self._trace.compute_qubits = len(self._q_to_id)
        return self._trace

    @property
    def block(self) -> Block:
        return self._blocks[-1]

    @property
    def q_to_id(self) -> _QidToTraceId:
        return self._q_to_id

    @property
    def classical_control_probability(self) -> float:
        return self._classical_control_probability

    @property
    def decomp_context(self) -> cirq.DecompositionContext:
        return self._decomp_context

    def handle_op(
        self,
        op: (
            cirq.Operation
            | tuple[int, list[cirq.Qid] | cirq.Qid]
            | tuple[int, list[cirq.Qid] | cirq.Qid, list[float] | float]
            | PushBlock
            | PopBlock
        ),
    ) -> None:
        """Recursively convert a single operation into trace instructions.

        Supported operation forms:

        - ``tuple``: A raw trace instruction as ``(id, qubits)`` or
        ``(id, qubits, params)``, added directly to the current block.
        - ``PushBlock`` / ``PopBlock``: Control block nesting with repetitions.
        - ``GateOperation``: Dispatched via ``_to_trace`` if available on the
        gate, otherwise decomposed via ``_decompose_with_context_`` or
        ``_decompose_``.
        - ``ClassicallyControlledOperation``: Included with the probability
        specified in the generation context.
        - ``list``: Each element is handled recursively.
        - Any other operation: Decomposed via ``_decompose_with_context_``.

        Args:
            op: The operation to convert.
        """
        if isinstance(op, tuple):
            if len(op) == 2:
                id, qubits = op
                params = None
            elif len(op) == 3:
                id, qubits, params = op

            qs = [
                self.q_to_id[q]
                for q in ([qubits] if isinstance(qubits, cirq.Qid) else qubits)
            ]

            if params is None:
                self.block.add_operation(id, qs)
            else:
                self.block.add_operation(
                    id, qs, params if isinstance(params, list) else [params]
                )
        elif isinstance(op, PushBlock):
            self.push_block(op.repetitions)
        elif isinstance(op, PopBlock):
            self.pop_block()
        elif isinstance(op, GateOperation):
            gate = op.gate

            if hasattr(gate, "_to_trace"):
                for sub_op in gate._to_trace(self.decomp_context, op):  # type: ignore
                    self.handle_op(sub_op)
            elif hasattr(gate, "_decompose_with_context_"):
                for sub_op in gate._decompose_with_context_(op.qubits, self.decomp_context):  # type: ignore
                    self.handle_op(sub_op)
            elif hasattr(gate, "_decompose_"):
                # decompose the gate and handle the resulting operations recursively
                for sub_op in gate._decompose_(op.qubits):  # type: ignore
                    self.handle_op(sub_op)
            else:
                for sub_op in op._decompose_with_context_(self.decomp_context):  # type: ignore
                    self.handle_op(sub_op)
        elif isinstance(op, ClassicallyControlledOperation):
            if random.random() < self.classical_control_probability:
                self.handle_op(op.without_classical_controls())
        elif isinstance(op, list):
            for sub_op in op:
                self.handle_op(sub_op)

        else:
            for sub_op in op._decompose_with_context_(self.decomp_context):  # type: ignore
                self.handle_op(sub_op)


@dataclass(frozen=True, slots=True)
class PushBlock:
    """Signals the start of a repeated block in the trace.

    Args:
        repetitions: Number of times the block is repeated.
    """

    repetitions: int


@dataclass(frozen=True, slots=True)
class PopBlock:
    """Signals the end of the current repeated block in the trace."""

    ...


class _QidToTraceId(dict):
    """Mapping from Cirq qubits to integer trace qubit indices.

    Initialized with a set of known qubits. If an unknown qubit is looked
    up, it is automatically assigned the next available index.
    """

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
    if _approx_eq(abs(self.exponent), 1):
        yield (H, [op.qubits[0]])
    else:
        yield from op._decompose_with_context_(context)  # type: ignore


def x_pow_gate_to_trace(self, context: cirq.DecompositionContext, op: cirq.Operation):
    q = [op.qubits[0]]
    exp = self.exponent
    if _approx_eq(exp, 1) or _approx_eq(exp, -1):
        yield (PAULI_X, q)
    elif _approx_eq(exp, 0.5):
        yield (SQRT_X, q)
    elif _approx_eq(exp, -0.5):
        yield (SQRT_X_DAG, q)
    elif _approx_eq(exp, 0.25):
        yield (SQRT_SQRT_X, q)
    elif _approx_eq(exp, -0.25):
        yield (SQRT_SQRT_X_DAG, q)
    else:
        yield (RX, q, exp * pi)


def y_pow_gate_to_trace(self, context: cirq.DecompositionContext, op: cirq.Operation):
    q = [op.qubits[0]]
    exp = self.exponent
    if _approx_eq(exp, 1) or _approx_eq(exp, -1):
        yield (PAULI_Y, q)
    elif _approx_eq(exp, 0.5):
        yield (SQRT_Y, q)
    elif _approx_eq(exp, -0.5):
        yield (SQRT_Y_DAG, q)
    elif _approx_eq(exp, 0.25):
        yield (SQRT_SQRT_Y, q)
    elif _approx_eq(exp, -0.25):
        yield (SQRT_SQRT_Y_DAG, q)
    else:
        yield (RY, q, exp * pi)


def z_pow_gate_to_trace(self, context: cirq.DecompositionContext, op: cirq.Operation):
    q = [op.qubits[0]]
    exp = self.exponent
    if _approx_eq(exp, 1) or _approx_eq(exp, -1):
        yield (PAULI_Z, q)
    elif _approx_eq(exp, 0.5):
        yield (S, q)
    elif _approx_eq(exp, -0.5):
        yield (S_DAG, q)
    elif _approx_eq(exp, 0.25):
        yield (T, q)
    elif _approx_eq(exp, -0.25):
        yield (T_DAG, q)
    else:
        yield (RZ, q, exp * pi)


def cx_pow_gate_to_trace(self, context: cirq.DecompositionContext, op: cirq.Operation):
    if _approx_eq(abs(self.exponent), 1):
        yield (CX, [op.qubits[0], op.qubits[1]])
    else:
        yield from op._decompose_with_context_(context)  # type: ignore


def cz_pow_gate_to_trace(self, context: cirq.DecompositionContext, op: cirq.Operation):
    exp = self.exponent
    c, t = op.qubits[0], op.qubits[1]
    if _approx_eq(abs(exp), 1):
        yield (CZ, [c, t])
    elif _approx_eq(exp, 0.5):
        # controlled S gate
        yield (T, [c])
        yield (T, [t])
        yield (CZ, [c, t])
        yield (T_DAG, [t])
        yield (CZ, [c, t])
    elif _approx_eq(exp, -0.5):
        # controlled S† gate
        yield (T_DAG, [c])
        yield (T_DAG, [t])
        yield (CZ, [c, t])
        yield (T, [t])
        yield (CZ, [c, t])
    else:
        rads = exp / 2 * pi
        yield (RZ, [c], [rads])
        yield (RZ, [t], [rads])
        yield (CZ, [c, t])
        yield (RZ, [t], [-rads])
        yield (CZ, [c, t])


def swap_pow_gate_to_trace(
    self, context: cirq.DecompositionContext, op: cirq.Operation
):
    if _approx_eq(abs(self.exponent), 1):
        yield (SWAP, [op.qubits[0], op.qubits[1]])
    else:
        yield from op._decompose_with_context_(context)  # type: ignore


def ccx_pow_gate_to_trace(self, context: cirq.DecompositionContext, op: cirq.Operation):
    if _approx_eq(abs(self.exponent), 1):
        yield (CCX, [op.qubits[0], op.qubits[1], op.qubits[2]])
    else:
        yield from op._decompose_with_context_(context)  # type: ignore


def ccz_pow_gate_to_trace(self, context: cirq.DecompositionContext, op: cirq.Operation):
    if _approx_eq(abs(self.exponent), 1):
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
    yield from ()


# Attach _to_trace methods to Cirq gate classes so that handle_op can
# convert them directly into trace instructions without decomposition.
HPowGate._to_trace = h_pow_gate_to_trace
XPowGate._to_trace = x_pow_gate_to_trace
YPowGate._to_trace = y_pow_gate_to_trace
ZPowGate._to_trace = z_pow_gate_to_trace
CXPowGate._to_trace = cx_pow_gate_to_trace
CZPowGate._to_trace = cz_pow_gate_to_trace
SwapPowGate._to_trace = swap_pow_gate_to_trace
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
