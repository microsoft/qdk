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
    circuit: cirq.CIRCUIT_LIKE, *, classical_control_probability: float = 0.5
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

    if isinstance(circuit, cirq.Circuit):
        # circuit is already in the expected format, so we can process it directly.
        pass
    elif isinstance(circuit, cirq.Gate):
        circuit = cirq.Circuit(circuit.on(*cirq.LineQid.for_gate(circuit)))
    else:
        # circuit is OP_TREE
        circuit = cirq.Circuit(circuit)

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
        op: cirq.OP_TREE | TraceGate | PushBlock | PopBlock,
    ) -> None:
        """Recursively convert a single operation into trace instructions.

        Supported operation forms:

        - ``TraceGate``: A raw trace instruction, added directly to the current block.
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
        if isinstance(op, TraceGate):
            qs = [
                self.q_to_id[q]
                for q in ([op.qubits] if isinstance(op.qubits, cirq.Qid) else op.qubits)
            ]

            if op.params is None:
                self.block.add_operation(op.id, qs)
            else:
                self.block.add_operation(
                    op.id, qs, op.params if isinstance(op.params, list) else [op.params]
                )
        elif isinstance(op, PushBlock):
            self.push_block(op.repetitions)
        elif isinstance(op, PopBlock):
            self.pop_block()
        elif isinstance(op, cirq.Operation):
            if isinstance(op, GateOperation):
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
            else:
                for sub_op in op._decompose_with_context_(self.decomp_context):  # type: ignore
                    self.handle_op(sub_op)
        else:
            # op is Iterable[OP_TREE]
            for sub_op in op:
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


@dataclass(frozen=True, slots=True)
class TraceGate:
    id: int
    qubits: list[cirq.Qid] | cirq.Qid
    params: list[float] | float | None = None


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
        yield TraceGate(H, [op.qubits[0]])
    else:
        yield from op._decompose_with_context_(context)  # type: ignore


def x_pow_gate_to_trace(self, context: cirq.DecompositionContext, op: cirq.Operation):
    q = [op.qubits[0]]
    exp = self.exponent
    if _approx_eq(exp, 1) or _approx_eq(exp, -1):
        yield TraceGate(PAULI_X, q)
    elif _approx_eq(exp, 0.5):
        yield TraceGate(SQRT_X, q)
    elif _approx_eq(exp, -0.5):
        yield TraceGate(SQRT_X_DAG, q)
    elif _approx_eq(exp, 0.25):
        yield TraceGate(SQRT_SQRT_X, q)
    elif _approx_eq(exp, -0.25):
        yield TraceGate(SQRT_SQRT_X_DAG, q)
    else:
        yield TraceGate(RX, q, exp * pi)


def y_pow_gate_to_trace(self, context: cirq.DecompositionContext, op: cirq.Operation):
    q = [op.qubits[0]]
    exp = self.exponent
    if _approx_eq(exp, 1) or _approx_eq(exp, -1):
        yield TraceGate(PAULI_Y, q)
    elif _approx_eq(exp, 0.5):
        yield TraceGate(SQRT_Y, q)
    elif _approx_eq(exp, -0.5):
        yield TraceGate(SQRT_Y_DAG, q)
    elif _approx_eq(exp, 0.25):
        yield TraceGate(SQRT_SQRT_Y, q)
    elif _approx_eq(exp, -0.25):
        yield TraceGate(SQRT_SQRT_Y_DAG, q)
    else:
        yield TraceGate(RY, q, exp * pi)


def z_pow_gate_to_trace(self, context: cirq.DecompositionContext, op: cirq.Operation):
    q = [op.qubits[0]]
    exp = self.exponent
    if _approx_eq(exp, 1) or _approx_eq(exp, -1):
        yield TraceGate(PAULI_Z, q)
    elif _approx_eq(exp, 0.5):
        yield TraceGate(S, q)
    elif _approx_eq(exp, -0.5):
        yield TraceGate(S_DAG, q)
    elif _approx_eq(exp, 0.25):
        yield TraceGate(T, q)
    elif _approx_eq(exp, -0.25):
        yield TraceGate(T_DAG, q)
    else:
        yield TraceGate(RZ, q, exp * pi)


def cx_pow_gate_to_trace(self, context: cirq.DecompositionContext, op: cirq.Operation):
    if _approx_eq(abs(self.exponent), 1):
        yield TraceGate(CX, [op.qubits[0], op.qubits[1]])
    else:
        yield from op._decompose_with_context_(context)  # type: ignore


def cz_pow_gate_to_trace(self, context: cirq.DecompositionContext, op: cirq.Operation):
    exp = self.exponent
    c, t = op.qubits[0], op.qubits[1]
    if _approx_eq(abs(exp), 1):
        yield TraceGate(CZ, [c, t])
    elif _approx_eq(exp, 0.5):
        # controlled S gate
        yield TraceGate(T, [c])
        yield TraceGate(T, [t])
        yield TraceGate(CZ, [c, t])
        yield TraceGate(T_DAG, [t])
        yield TraceGate(CZ, [c, t])
    elif _approx_eq(exp, -0.5):
        # controlled S† gate
        yield TraceGate(T_DAG, [c])
        yield TraceGate(T_DAG, [t])
        yield TraceGate(CZ, [c, t])
        yield TraceGate(T, [t])
        yield TraceGate(CZ, [c, t])
    else:
        rads = exp / 2 * pi
        yield TraceGate(RZ, [c], [rads])
        yield TraceGate(RZ, [t], [rads])
        yield TraceGate(CZ, [c, t])
        yield TraceGate(RZ, [t], [-rads])
        yield TraceGate(CZ, [c, t])


def swap_pow_gate_to_trace(
    self, context: cirq.DecompositionContext, op: cirq.Operation
):
    if _approx_eq(abs(self.exponent), 1):
        yield TraceGate(SWAP, [op.qubits[0], op.qubits[1]])
    else:
        yield from op._decompose_with_context_(context)  # type: ignore


def ccx_pow_gate_to_trace(self, context: cirq.DecompositionContext, op: cirq.Operation):
    if _approx_eq(abs(self.exponent), 1):
        yield TraceGate(CCX, [op.qubits[0], op.qubits[1], op.qubits[2]])
    else:
        yield from op._decompose_with_context_(context)  # type: ignore


def ccz_pow_gate_to_trace(self, context: cirq.DecompositionContext, op: cirq.Operation):
    if _approx_eq(abs(self.exponent), 1):
        yield TraceGate(CCZ, [op.qubits[0], op.qubits[1], op.qubits[2]])
    else:
        yield from op._decompose_with_context_(context)  # type: ignore


def measurement_gate_to_trace(
    self, context: cirq.DecompositionContext, op: cirq.Operation
):
    for q in op.qubits:
        yield TraceGate(MEAS_Z, [q])


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
