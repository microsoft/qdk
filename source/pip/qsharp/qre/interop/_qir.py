# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations

from collections import OrderedDict

import pyqir

from ..._simulation import AggregateGatesPass
from ..._native import QirInstructionId
from .._qre import Trace
from .. import instruction_ids as ids

# Maps QirInstructionId to (instruction_id, arity) where arity is:
#   1 = single-qubit gate: tuple is (op, qubit)
#   2 = two-qubit gate: tuple is (op, qubit1, qubit2)
#   3 = three-qubit gate: tuple is (op, qubit1, qubit2, qubit3)
#  -1 = single-qubit rotation: tuple is (op, angle, qubit)
#  -2 = two-qubit rotation: tuple is (op, angle, qubit1, qubit2)
_GATE_MAP: list[tuple[QirInstructionId, int, int]] = [
    # Single-qubit gates
    (QirInstructionId.I, ids.PAULI_I, 1),
    (QirInstructionId.H, ids.H, 1),
    (QirInstructionId.X, ids.PAULI_X, 1),
    (QirInstructionId.Y, ids.PAULI_Y, 1),
    (QirInstructionId.Z, ids.PAULI_Z, 1),
    (QirInstructionId.S, ids.S, 1),
    (QirInstructionId.SAdj, ids.S_DAG, 1),
    (QirInstructionId.SX, ids.SQRT_X, 1),
    (QirInstructionId.SXAdj, ids.SQRT_X_DAG, 1),
    (QirInstructionId.T, ids.T, 1),
    (QirInstructionId.TAdj, ids.T_DAG, 1),
    # Two-qubit gates
    (QirInstructionId.CNOT, ids.CNOT, 2),
    (QirInstructionId.CX, ids.CX, 2),
    (QirInstructionId.CY, ids.CY, 2),
    (QirInstructionId.CZ, ids.CZ, 2),
    (QirInstructionId.SWAP, ids.SWAP, 2),
    # Three-qubit gates
    (QirInstructionId.CCX, ids.CCX, 3),
    # Single-qubit rotations (op, angle, qubit)
    (QirInstructionId.RX, ids.RX, -1),
    (QirInstructionId.RY, ids.RY, -1),
    (QirInstructionId.RZ, ids.RZ, -1),
    # Two-qubit rotations (op, angle, qubit1, qubit2)
    (QirInstructionId.RXX, ids.RXX, -2),
    (QirInstructionId.RYY, ids.RYY, -2),
    (QirInstructionId.RZZ, ids.RZZ, -2),
]

_MEAS_MAP: list[tuple[QirInstructionId, int]] = [
    (QirInstructionId.M, ids.MEAS_Z),
    (QirInstructionId.MZ, ids.MEAS_Z),
    (QirInstructionId.MResetZ, ids.MEAS_RESET_Z),
]

_SKIP = (
    # Resets qubit to |0⟩ without measuring; we do not currently account for
    # that in resource estimation
    QirInstructionId.RESET,
    # Runtime qubit state transfer; an implementation detail, not a logical operation
    QirInstructionId.Move,
    # Reads a measurement result from classical memory; purely classical I/O
    QirInstructionId.ReadResult,
    # The following are classical output recording operations that do not represent
    # quantum operations and have no impact on resource estimation.
    QirInstructionId.ResultRecordOutput,
    QirInstructionId.BoolRecordOutput,
    QirInstructionId.IntRecordOutput,
    QirInstructionId.DoubleRecordOutput,
    QirInstructionId.TupleRecordOutput,
    QirInstructionId.ArrayRecordOutput,
)


class _FalseBranchGatesPass(AggregateGatesPass):
    """Extends AggregateGatesPass to handle conditional branches by always
    following the false branch (assuming measurement results are Zero)."""

    def _on_function(self, function: pyqir.Function) -> None:
        if not function.basic_blocks:
            return

        # Walk blocks, taking only the false successor at conditional branches.
        visited: OrderedDict[pyqir.BasicBlock, pyqir.BasicBlock] = OrderedDict()
        queue = [function.basic_blocks[0]]

        while queue:
            block = queue.pop(0)
            if block in visited:
                continue
            visited[block] = block

            term = block.terminator
            if term is not None:
                succs = term.successors
                if term.opcode == pyqir.Opcode.BR and len(term.operands) > 1:
                    # Conditional branch: follow the false path (result == Zero)
                    queue.append(succs[1])
                else:
                    queue.extend(succs)

        for block in visited.values():
            self._on_block(block)

    def _on_block(self, block: pyqir.BasicBlock) -> None:
        # Bypass AggregateGatesPass's error on conditional branches;
        # call the grandparent implementation that just visits instructions.
        pyqir.QirModuleVisitor._on_block(self, block)

    def _on_call_instr(self, call: pyqir.Call) -> None:
        # The base AggregateGatesPass only handles programs without branching,
        # so it never encounters these intrinsics.  In branching programs,
        # read_result reads a measurement outcome to use as a branch condition,
        # and bool/int_record_output write computed (non-constant) classical
        # values.  We handle them here because their first argument may be an
        # SSA variable rather than a constant, which the base class expects.
        callee_name = call.callee.name
        if callee_name == "__quantum__rt__read_result":
            self.gates.append(
                (QirInstructionId.ReadResult, pyqir.result_id(call.args[0]))
            )
        elif callee_name == "__quantum__rt__bool_record_output":
            tag = self._get_value_as_string(call.args[1])
            self.gates.append((QirInstructionId.BoolRecordOutput, "0", tag))
        elif callee_name == "__quantum__rt__int_record_output":
            tag = self._get_value_as_string(call.args[1])
            self.gates.append((QirInstructionId.IntRecordOutput, "0", tag))
        else:
            super()._on_call_instr(call)


def trace_from_qir(input: str | bytes) -> Trace:
    """Convert a QIR program into a resource-estimation Trace.

    Parses the QIR module, extracts quantum gates, and builds a Trace that
    can be used for resource estimation.  Conditional branches are resolved
    by always following the false path (assuming measurement results are Zero).

    Args:
        input: QIR input as LLVM IR text (str) or bitcode (bytes).

    Returns:
        A Trace containing the quantum operations from the QIR program.
    """
    context = pyqir.Context()

    if isinstance(input, str):
        mod = pyqir.Module.from_ir(context, input)
    else:
        mod = pyqir.Module.from_bitcode(context, input)

    gates, num_qubits, _ = _FalseBranchGatesPass().run(mod)

    trace = Trace(compute_qubits=num_qubits)

    for gate in gates:
        # NOTE: AggregateGatesPass does not return QirInstruction objects
        assert isinstance(gate, tuple)
        _add_gate(trace, gate)

    return trace


def _add_gate(trace: Trace, gate: tuple) -> None:
    op = gate[0]

    for qir_id, instr_id, arity in _GATE_MAP:
        if op == qir_id:
            if arity == 1:
                trace.add_operation(instr_id, [gate[1]])
            elif arity == 2:
                trace.add_operation(instr_id, [gate[1], gate[2]])
            elif arity == 3:
                trace.add_operation(instr_id, [gate[1], gate[2], gate[3]])
            elif arity == -1:
                trace.add_operation(instr_id, [gate[2]], [gate[1]])
            elif arity == -2:
                trace.add_operation(instr_id, [gate[2], gate[3]], [gate[1]])
            return

    for qir_id, instr_id in _MEAS_MAP:
        if op == qir_id:
            trace.add_operation(instr_id, [gate[1]])
            return

    for skip_id in _SKIP:
        if op == skip_id:
            return

    # The only unhandled QirInstructionId is CorrelatedNoise
    assert op == QirInstructionId.CorrelatedNoise, f"Unexpected QIR instruction: {op}"
    raise NotImplementedError(f"Unsupported QIR instruction: {op}")
