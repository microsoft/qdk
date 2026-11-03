# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Structural conversion between Qiskit circuits and :mod:`qdk.circuit`.

Nothing here goes through OpenQASM, QIR, or any other text. Instructions are
read from and written to circuit objects directly, because a text round trip
cannot carry a large circuit at a workable speed and loses whatever the text
format has no syntax for.

Qiskit is the reference for what a Qiskit gate means. That matters more than it
sounds: Qiskit's ``u`` follows a superseded revision of the OpenQASM
specification and differs from the current one by a global phase of
``exp(i*theta/2)``. Reading a Qiskit circuit therefore uses Qiskit's own
convention, not the one the Q# standard library implements.
"""

from __future__ import annotations

import math
from typing import (
    TYPE_CHECKING,
    Any,
    Callable,
    ContextManager,
    Dict,
    List,
    Optional,
    Tuple,
    cast,
)

from ..circuit import ClassicalRegister, QuantumCircuit, QuantumRegister

if TYPE_CHECKING:  # pragma: no cover - import cycle only matters for typing
    from qiskit import QuantumCircuit as QiskitCircuit


class ConversionError(Exception):
    """Raised when a circuit holds something the other side cannot express."""


# Gates whose meaning and operand order agree exactly, mapped to our method.
_DIRECT: Dict[str, str] = {
    "id": "i",
    "x": "x",
    "y": "y",
    "z": "z",
    "h": "h",
    "s": "s",
    "sdg": "sdg",
    "sx": "sx",
    "sxdg": "sxdg",
    "t": "t",
    "tdg": "tdg",
    "cx": "cx",
    "cy": "cy",
    "cz": "cz",
    "swap": "swap",
    "ccx": "ccx",
    "cswap": "cswap",
}

# Rotations, which carry exactly one angle ahead of their qubits.
_ROTATIONS: Dict[str, str] = {
    "rx": "rx",
    "ry": "ry",
    "rz": "rz",
    "rxx": "rxx",
    "ryy": "ryy",
    "rzz": "rzz",
    "crx": "crx",
    "cry": "cry",
    "crz": "crz",
}


def _emit_u(circuit: QuantumCircuit, qubits: List[int], params: List[Any]) -> float:
    """Writes Qiskit's ``u`` and returns the phase it leaves behind.

    ``u`` is a leaf rather than something expanded through Qiskit's own
    definition, because that definition is written in terms of ``u`` itself.
    """
    theta, phi, lam = params
    circuit.rz(lam, qubits[0])
    circuit.ry(theta, qubits[0])
    circuit.rz(phi, qubits[0])
    return (phi + lam) / 2


def _emit_p(circuit: QuantumCircuit, qubits: List[int], params: List[Any]) -> float:
    """Writes Qiskit's ``p`` and returns the phase it leaves behind."""
    (lam,) = params
    circuit.rz(lam, qubits[0])
    return lam / 2


_LEAVES: Dict[str, Callable[[QuantumCircuit, List[int], List[Any]], float]] = {
    "u": _emit_u,
    "u3": _emit_u,
    "p": _emit_p,
    "u1": _emit_p,
    "phase": _emit_p,
}


def _resolved(value: Any) -> Any:
    """Reduces a Qiskit parameter expression to a number when it is one."""
    if isinstance(value, (int, float)):
        return float(value)
    try:
        return float(value)
    except (TypeError, ValueError):
        return value


_MAX_DEPTH = 32

_CONTROL_FLOW = frozenset({"if_else", "while_loop", "for_loop", "box", "switch_case"})


class _Context:
    """What reading an instruction needs beyond the instruction itself.

    A control-flow condition names bits of the circuit the construct sits in
    rather than bits of its own body, so the outer mapping has to travel with
    the walk.
    """

    def __init__(self, symbols: Dict[Any, Any], clbit_index: Dict[Any, int]):
        self.symbols = symbols
        self.clbit_index = clbit_index

    def with_symbols(self, symbols: Dict[Any, Any]) -> "_Context":
        return _Context(symbols, self.clbit_index)


def _translate(value: Any, bound: Dict[Any, Any], gate: str) -> Any:
    """Resolves a Qiskit parameter value into a number or one of our symbols."""
    resolved = _resolved(value)
    if isinstance(resolved, float):
        return resolved
    symbol = bound.get(resolved)
    if symbol is not None:
        return symbol
    raise ConversionError(
        f"gate '{gate}' carries the expression '{value}', and only a plain "
        "parameter or a number is understood"
    )


def _register_layout(
    source: "QiskitCircuit",
    bits: Any,
    registers: Any,
    kind: str,
) -> Tuple[Dict[Any, int], List[Tuple[str, int]], int]:
    """Maps Qiskit bits to our indices, and its registers to ours.

    Our registers tile their index space in order and never overlap, which is
    what makes an index enough to name a bit. Qiskit allows a bit to sit in
    several registers, so a circuit relying on that is refused rather than
    quietly flattened into something that reads the same but is not.
    """
    index_of = {bit: position for position, bit in enumerate(bits)}
    placed = []
    covered = 0
    for register in registers:
        positions = [index_of[bit] for bit in register]
        if positions != list(range(covered, covered + len(positions))):
            raise ConversionError(
                f"{kind} register '{register.name}' does not cover a contiguous "
                "range of bits in declaration order, which this circuit cannot "
                "represent"
            )
        covered += len(positions)
        placed.append((register.name, len(positions)))
    return index_of, placed, covered


def from_qiskit(source: "QiskitCircuit") -> QuantumCircuit:
    """Builds our circuit from a Qiskit one.

    Raises:
        ConversionError: when the source holds something with no counterpart.
    """
    from qiskit.circuit import Parameter as QkParameter

    qubit_index, qregs, qcovered = _register_layout(
        source, source.qubits, source.qregs, "quantum"
    )
    clbit_index, cregs, ccovered = _register_layout(
        source, source.clbits, source.cregs, "classical"
    )

    specs: List[Any] = [QuantumRegister(name, size) for name, size in qregs]
    specs += [ClassicalRegister(name, size) for name, size in cregs]
    if specs:
        target = QuantumCircuit.from_registers(
            *specs, clbits=len(source.clbits) - ccovered
        )
        # Loose qubits sit past the named ones, which from_registers cannot add.
        if len(source.qubits) > qcovered:
            raise ConversionError(
                "circuit has qubits outside any register, which this circuit "
                "cannot represent alongside named registers"
            )
    else:
        target = QuantumCircuit(len(source.qubits), len(source.clbits))

    symbols: Dict[Any, Any] = {}
    for parameter in source.parameters:
        if not isinstance(parameter, QkParameter):
            raise ConversionError(f"parameter '{parameter}' is not a plain symbol")
        symbols[parameter] = target.parameter(parameter.name)
    context = _Context(symbols, clbit_index)

    phase = _as_number(source.global_phase, "global phase")
    for instruction in source.data:
        phase += _emit(
            target,
            instruction.operation,
            [qubit_index[bit] for bit in instruction.qubits],
            [clbit_index[bit] for bit in instruction.clbits],
            0,
            context,
        )
    target.global_phase = phase
    return target


def _as_number(value: Any, what: str) -> float:
    resolved = _resolved(value)
    if not isinstance(resolved, float):
        raise ConversionError(f"{what} is still symbolic and cannot be resolved")
    return resolved


def _emit(
    circuit: QuantumCircuit,
    operation: Any,
    qubits: List[int],
    clbits: List[int],
    depth: int,
    context: _Context,
) -> float:
    """Writes one Qiskit operation, returning the phase it leaves behind."""
    name = operation.name
    if name in _CONTROL_FLOW:
        return _emit_control(circuit, operation, qubits, clbits, depth, context)
    params = [_translate(value, context.symbols, name) for value in operation.params]

    if name in _DIRECT:
        getattr(circuit, _DIRECT[name])(*qubits)
        return 0.0
    if name in _ROTATIONS:
        getattr(circuit, _ROTATIONS[name])(params[0], *qubits)
        return 0.0
    if name in _LEAVES:
        return _LEAVES[name](circuit, qubits, params)
    if name == "measure":
        circuit.measure(qubits[0], clbits[0])
        return 0.0
    if name == "reset":
        circuit.reset(qubits[0])
        return 0.0
    if name == "barrier":
        circuit.barrier(qubits)
        return 0.0
    if name == "global_phase":
        return float(params[0]) if params else 0.0
    if name == "delay":
        raise ConversionError("delay has no conversion to this circuit yet")

    return _expand(circuit, operation, qubits, clbits, depth, context)


def _condition(
    circuit: QuantumCircuit, condition: Any, context: _Context
) -> Tuple[Any, int]:
    """Reads a Qiskit condition as a subject and the value it is tested against."""
    from qiskit import ClassicalRegister as QkClassical
    from qiskit.circuit import Clbit
    from qiskit.circuit.classical import expr

    if isinstance(condition, tuple):
        subject, value = condition
    elif isinstance(condition, expr.Binary) and condition.op is expr.Binary.Op.EQUAL:
        # The modern spelling of the same test, which carries its operands as an
        # expression tree rather than as a pair.
        if not (
            isinstance(condition.left, expr.Var)
            and isinstance(condition.right, expr.Value)
        ):
            raise ConversionError(
                "only a comparison of a register or bit against a constant is "
                "understood as a condition"
            )
        subject, value = condition.left.var, condition.right.value
    else:
        raise ConversionError(
            "condition is a classical expression beyond a single equality, which "
            "this circuit cannot represent"
        )

    if isinstance(subject, Clbit):
        return context.clbit_index[subject], int(value)
    if isinstance(subject, QkClassical):
        for register in circuit.classical_registers:
            if register.name == subject.name:
                return register, int(value)
        raise ConversionError(
            f"condition names register '{subject.name}', which did not survive "
            "conversion"
        )
    raise ConversionError(f"condition tests '{subject}', which has no counterpart")


def _emit_body(
    circuit: QuantumCircuit,
    body: "QiskitCircuit",
    qubits: List[int],
    clbits: List[int],
    depth: int,
    context: _Context,
) -> float:
    """Writes the instructions of a control-flow body into the open block.

    A body addresses its own bits, which stand for the ones the construct was
    given, so the mapping is positional.
    """
    phase = _as_number(body.global_phase, "phase of a control-flow body")
    if phase != 0.0:
        # Our constructs have nowhere to record a phase that applies only when a
        # branch is taken, and folding it into the circuit's own phase would
        # apply it unconditionally.
        raise ConversionError(
            "a control-flow body carries its own global phase, which this "
            "circuit cannot represent"
        )
    into_qubits = {bit: qubits[position] for position, bit in enumerate(body.qubits)}
    into_clbits = {bit: clbits[position] for position, bit in enumerate(body.clbits)}
    for instruction in body.data:
        phase += _emit(
            circuit,
            instruction.operation,
            [into_qubits[bit] for bit in instruction.qubits],
            [into_clbits[bit] for bit in instruction.clbits],
            depth + 1,
            context,
        )
    return phase


def _emit_control(
    circuit: QuantumCircuit,
    operation: Any,
    qubits: List[int],
    clbits: List[int],
    depth: int,
    context: _Context,
) -> float:
    """Writes a control-flow construct and everything inside it."""
    if depth >= _MAX_DEPTH:
        raise ConversionError("control flow is nested more deeply than supported")
    name = operation.name

    if name == "box":
        (body,) = operation.params
        with circuit.box_():
            return _emit_body(circuit, body, qubits, clbits, depth, context)

    if name == "for_loop":
        indexset, loop_parameter, body = operation.params
        values = [int(value) for value in indexset]
        named = loop_parameter.name if loop_parameter is not None else None
        with circuit.for_loop(values, named) as index:
            inner = context
            if loop_parameter is not None:
                symbols = dict(context.symbols)
                symbols[loop_parameter] = index
                inner = context.with_symbols(symbols)
            return _emit_body(circuit, body, qubits, clbits, depth, inner)

    if name == "while_loop":
        subject, value = _condition(circuit, operation.condition, context)
        (body,) = operation.params
        with circuit.while_loop(subject, value):
            return _emit_body(circuit, body, qubits, clbits, depth, context)

    if name == "if_else":
        subject, value = _condition(circuit, operation.condition, context)
        true_body, false_body = operation.params
        builder = circuit.if_test(subject, value)
        phase = 0.0
        with builder:
            phase += _emit_body(circuit, true_body, qubits, clbits, depth, context)
        if false_body is not None:
            with builder.otherwise():
                phase += _emit_body(circuit, false_body, qubits, clbits, depth, context)
        return phase

    raise ConversionError(
        f"control-flow construct '{name}' has no counterpart in this circuit"
    )


def _expand(
    circuit: QuantumCircuit,
    operation: Any,
    qubits: List[int],
    clbits: List[int],
    depth: int,
    context: _Context,
) -> float:
    """Rewrites a gate through the definition Qiskit gives for it.

    Taking the rewrite from Qiskit rather than writing one per gate keeps the
    result true to what the source library means by that gate, and covers the
    synthesised gates whose expansion depends on how many controls they carry.
    """
    if depth >= _MAX_DEPTH:
        raise ConversionError(
            f"gate '{operation.name}' is still not expressible after "
            f"{_MAX_DEPTH} rewrites"
        )
    definition = getattr(operation, "definition", None)
    if definition is None:
        raise ConversionError(
            f"gate '{operation.name}' has no counterpart and no definition to "
            "rewrite it with"
        )
    if definition.clbits:
        raise ConversionError(
            f"gate '{operation.name}' expands into something using classical "
            "bits, which is not supported"
        )

    local = {bit: qubits[position] for position, bit in enumerate(definition.qubits)}
    phase = _as_number(definition.global_phase, f"phase of gate '{operation.name}'")
    for instruction in definition.data:
        phase += _emit(
            circuit,
            instruction.operation,
            [local[bit] for bit in instruction.qubits],
            [],
            depth + 1,
            context,
        )
    return phase


# Our gates that Qiskit names differently, or not at all.
_TO_QISKIT: Dict[str, str] = {"i": "id", "measure_z": "measure"}

# Basis measurements and preparations, which Qiskit has no gate for. The
# rotations carry the observable onto Z and back, matching how this circuit
# lowers them elsewhere, so the qubit is left in the observed eigenstate.
_BASIS_ROTATION: Dict[str, Tuple[Tuple[str, ...], Tuple[str, ...]]] = {
    "measure_x": (("h",), ("h",)),
    "measure_y": (("sdg", "h"), ("h", "s")),
}


class _Writer:
    """Carries what writing an instruction needs beyond the instruction.

    Symbols are looked up by name, because a view reports a parameter by name
    and the Qiskit object standing for it is created once per conversion.
    """

    def __init__(self, source: QuantumCircuit, symbols: Dict[str, Any]):
        self.source = source
        self.symbols = symbols

    def with_symbol(self, name: str, symbol: Any) -> "_Writer":
        symbols = dict(self.symbols)
        symbols[name] = symbol
        return _Writer(self.source, symbols)


def to_qiskit(source: QuantumCircuit) -> "QiskitCircuit":
    """Builds a Qiskit circuit from ours.

    Raises:
        ConversionError: when the source holds something Qiskit cannot express.
    """
    from qiskit import ClassicalRegister as QkClassical
    from qiskit import QuantumCircuit as QkCircuit
    from qiskit import QuantumRegister as QkQuantum
    from qiskit.circuit import Parameter as QkParameter

    registers: List[Any] = [
        QkQuantum(register.size, register.name) for register in source.quantum_registers
    ]
    registers += [
        QkClassical(register.size, register.name)
        for register in source.classical_registers
    ]
    named_qubits = sum(r.size for r in source.quantum_registers)
    named_clbits = sum(r.size for r in source.classical_registers)
    if registers and (
        named_qubits != source.num_qubits or named_clbits != source.num_clbits
    ):
        raise ConversionError(
            "circuit mixes named registers with bits outside them, which has no "
            "unambiguous Qiskit form"
        )
    target = (
        QkCircuit(*registers)
        if registers
        else QkCircuit(source.num_qubits, source.num_clbits)
    )
    target.global_phase = source.global_phase

    symbols = {
        parameter.name: QkParameter(parameter.name) for parameter in source.parameters
    }
    writer = _Writer(source, symbols)
    for position in range(len(source)):
        _write(target, source[position], writer)
    return target


def _angle(value: Any, writer: _Writer, gate: str) -> Any:
    """Resolves a stored angle into something Qiskit accepts."""
    if isinstance(value, float):
        return value
    name = getattr(value, "name", None)
    if name is not None and name in writer.symbols:
        return writer.symbols[name]
    raise ConversionError(
        f"gate '{gate}' carries the angle '{value}', which has no Qiskit form"
    )


def _register_named(target: "QiskitCircuit", name: str) -> Any:
    for register in target.cregs:
        if register.name == name:
            return register
    raise ConversionError(f"condition names register '{name}', which is not present")


def _flatten_and(condition: Any) -> List[Any]:
    """Splits a chain of conjunctions into the tests it is built from."""
    if condition.kind == "binary" and condition.op == "and":
        return _flatten_and(condition.operands[0]) + _flatten_and(condition.operands[1])
    return [condition]


def _bit_test(condition: Any) -> Optional[Tuple[int, int]]:
    """Reads an equality between one classical bit and a constant."""
    if condition.kind != "binary" or condition.op != "==":
        return None
    left, right = condition.operands
    if left.kind != "clbit" or right.kind != "literal":
        return None
    value = right.value
    if isinstance(value, bool):
        return left.clbit, int(value)
    if isinstance(value, int):
        return left.clbit, value
    return None


def _write_condition(target: "QiskitCircuit", view: Any) -> Tuple[Any, int]:
    """Rebuilds a Qiskit condition from a stored one.

    A test of a whole register is stored as one equality per bit, so the bits
    are gathered back up and read as the number they spell.
    """
    condition = view.condition
    if condition is None:
        raise ConversionError(f"'{view.name}' has no condition to convert")

    tests = [_bit_test(part) for part in _flatten_and(condition)]
    if any(test is None for test in tests):
        raise ConversionError(
            "condition is not a comparison of classical bits against constants, "
            "which is the only form Qiskit can be given here"
        )
    resolved: List[Tuple[int, int]] = [test for test in tests if test is not None]

    if len(resolved) == 1:
        bit, value = resolved[0]
        return target.clbits[bit], value

    bits = [bit for bit, _ in resolved]
    if bits != list(range(bits[0], bits[0] + len(bits))):
        raise ConversionError(
            "condition tests classical bits that are not a contiguous run, so it "
            "does not name a register"
        )
    for register in target.cregs:
        start = target.clbits.index(register[0])
        if start == bits[0] and len(register) == len(bits):
            value = 0
            for offset, (_, bit_value) in enumerate(resolved):
                value |= (1 if bit_value else 0) << offset
            return register, value
    raise ConversionError(
        "condition tests a run of classical bits that is not a whole register"
    )


def _write_block(target: "QiskitCircuit", block: int, writer: _Writer) -> None:
    for view in writer.source.block(block):
        _write(target, view, writer)


def _write_control(target: "QiskitCircuit", view: Any, writer: _Writer) -> None:
    """Writes a control-flow construct and everything inside it."""
    name = view.name

    if name == "box":
        # Qiskit gives box one signature covering both the direct and the
        # context-manager form, so which one an empty annotation list selects is
        # only knowable at run time.
        opened = cast(ContextManager[Any], target.box([]))
        with opened:
            _write_block(target, view.blocks[0], writer)
        return

    if name == "for":
        values = [
            int(value) if isinstance(value, (int, bool)) else value
            for value in view.values
        ]
        with target.for_loop(values, None, None, None, None, label=None) as index:
            inner = writer
            if view.loop_index is not None:
                inner = writer.with_symbol(view.loop_index, index)
            _write_block(target, view.blocks[0], inner)
        return

    if name == "while":
        with target.while_loop(
            _write_condition(target, view), None, None, None, label=None
        ):
            _write_block(target, view.blocks[0], writer)
        return

    if name == "if_else":
        condition = _write_condition(target, view)
        if len(view.blocks) == 1:
            with target.if_test(condition):
                _write_block(target, view.blocks[0], writer)
            return
        with target.if_test(condition) as otherwise:
            _write_block(target, view.blocks[0], writer)
        with otherwise:
            _write_block(target, view.blocks[1], writer)
        return

    raise ConversionError(f"control-flow construct '{name}' has no Qiskit form")


def _write(target: "QiskitCircuit", view: Any, writer: _Writer) -> None:
    """Writes one of our instructions into a Qiskit circuit."""
    name: str = view.name
    if view.blocks:
        _write_control(target, view, writer)
        return

    qubits = list(view.qubits)
    clbits = list(view.clbits)
    params = [_angle(value, writer, name) for value in view.params]

    if name in _BASIS_ROTATION:
        before, after = _BASIS_ROTATION[name]
        for gate in before:
            getattr(target, gate)(qubits[0])
        target.measure(qubits[0], clbits[0])
        for gate in after:
            getattr(target, gate)(qubits[0])
        return
    if name in ("prepare_x", "prepare_y", "prepare_z"):
        target.reset(qubits[0])
        if name == "prepare_x":
            target.h(qubits[0])
        elif name == "prepare_y":
            target.h(qubits[0])
            target.s(qubits[0])
        return
    if name == "barrier":
        target.barrier(qubits)
        return
    if name == "delay":
        raise ConversionError("delay has no conversion to Qiskit yet")
    if name in ("break", "continue", "switch"):
        raise ConversionError(f"'{name}' has no Qiskit form here")

    method: str = _TO_QISKIT.get(name, name)
    if not hasattr(target, method):
        raise ConversionError(f"operation '{name}' has no Qiskit counterpart")
    getattr(target, method)(*params, *qubits, *clbits)
