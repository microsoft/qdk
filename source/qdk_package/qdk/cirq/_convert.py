# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Structural conversion between Cirq circuits and :mod:`qdk.circuit`.

Nothing here goes through OpenQASM or any other text.

Three differences between the two models drive the whole design.

Cirq identifies qubits by object rather than by position, and a circuit is a
sparse set of them with no declared width: a circuit using only
``LineQubit(5)`` is one qubit wide, not six. Conversion therefore has to choose
indices, which it does by sorted order, and ``to_cirq`` accepts the original
qubits back so the identities can be restored.

Cirq has no classical registers. A measurement carries a string key and may
cover several qubits, so each key becomes a classical register of that width.

Cirq schedules into moments, which assert simultaneity. Flattening them is
lossless only when Cirq's own earliest-first scheduling would rebuild the same
boundaries, so the converter checks rather than assumes.
"""

from __future__ import annotations

import cmath
import math
from typing import TYPE_CHECKING, Any, Dict, List, Optional, Sequence, Tuple

from ..circuit import ClassicalRegister, QuantumCircuit

if TYPE_CHECKING:  # pragma: no cover - only needed for typing
    import cirq


class ConversionError(Exception):
    """Raised when a circuit holds something the other side cannot express."""


# Cirq gates whose meaning and operand order match ours exactly, at exponent 1.
_DIRECT: Dict[str, str] = {
    "I": "i",
    "X": "x",
    "Y": "y",
    "Z": "z",
    "H": "h",
    "S": "s",
    "T": "t",
    "CNOT": "cx",
    "CX": "cx",
    "CZ": "cz",
    "SWAP": "swap",
    "TOFFOLI": "ccx",
    "CCX": "ccx",
    "CCNOT": "ccx",
    "FREDKIN": "cswap",
    "CSWAP": "cswap",
}

# Gates carrying one exponent, mapped to the rotation they are a power of.
#
# Cirq writes ``X**t`` as an eigengate with an exponent and a global shift.
# The rotation is ``pi * t`` about the same axis, and what is left over is a
# phase of ``pi * t * (shift + 1/2)``, which is recorded rather than dropped.
_POWERS: Dict[str, str] = {
    "XPowGate": "rx",
    "YPowGate": "ry",
    "ZPowGate": "rz",
}

_HALF_TURN = math.pi


def _sorted_qubits(circuit: "cirq.Circuit") -> List[Any]:
    """The circuit's qubits in the order their indices are assigned."""
    return sorted(circuit.all_qubits())


def _measurement_registers(circuit: "cirq.Circuit") -> List[Tuple[str, int]]:
    """Names and widths of the classical registers a circuit's keys need."""
    import cirq

    found: List[Tuple[str, int]] = []
    widths: Dict[str, int] = {}
    for moment in circuit:
        for operation in moment.operations:
            if not isinstance(operation.gate, cirq.MeasurementGate):
                continue
            gate: Any = operation.gate
            key: str = gate.key
            width = len(operation.qubits)
            if key in widths:
                if widths[key] != width:
                    raise ConversionError(
                        f"measurement key '{key}' covers {widths[key]} qubits in "
                        f"one place and {width} in another"
                    )
                continue
            widths[key] = width
            found.append((key, width))
    return found


def _moments_survive_flattening(circuit: "cirq.Circuit") -> bool:
    """Whether Cirq's own scheduling would rebuild the same moments.

    Flattening loses nothing when it would, which is the case for any circuit
    built by plain appending, so the boundaries are only recorded when the
    author put them somewhere earliest-first scheduling would not.
    """
    import cirq

    operations = [operation for moment in circuit for operation in moment.operations]
    rebuilt = cirq.Circuit(operations)
    return [set(m.operations) for m in rebuilt] == [set(m.operations) for m in circuit]


def from_cirq(
    source: "cirq.Circuit", preserve_moments: Optional[bool] = None
) -> QuantumCircuit:
    """Builds our circuit from a Cirq one.

    Qubits are indexed by sorted order. Pass the same order back to
    :func:`to_cirq` to restore the original qubit objects.

    ``preserve_moments`` records each moment as a moment construct. Left as
    ``None`` it is decided per circuit, recording them only when flattening
    would move an operation into a different moment.

    Raises:
        ConversionError: when the source holds something with no counterpart.
    """
    from ..circuit import QuantumRegister

    qubits = _sorted_qubits(source)
    index_of = {qubit: position for position, qubit in enumerate(qubits)}
    registers = _measurement_registers(source)

    specs: List[Any] = [QuantumRegister("q", max(len(qubits), 1))]
    specs += [ClassicalRegister(name, width) for name, width in registers]
    target = QuantumCircuit.from_registers(*specs)

    clbit_of: Dict[str, int] = {}
    offset = 0
    for name, width in registers:
        clbit_of[name] = offset
        offset += width

    if preserve_moments is None:
        preserve_moments = not _moments_survive_flattening(source)

    phase = 0.0
    for moment in source:
        if preserve_moments:
            with target.moment():
                phase += _emit_moment(target, moment, index_of, clbit_of)
        else:
            phase += _emit_moment(target, moment, index_of, clbit_of)
    target.global_phase = phase
    return target


def _emit_moment(
    target: QuantumCircuit,
    moment: Any,
    index_of: Dict[Any, int],
    clbit_of: Dict[str, int],
) -> float:
    phase = 0.0
    for operation in moment.operations:
        phase += _emit(target, operation, index_of, clbit_of, 0)
    return phase


_MAX_DEPTH = 32


def _emit(
    target: QuantumCircuit,
    operation: Any,
    index_of: Dict[Any, int],
    clbit_of: Dict[str, int],
    depth: int,
) -> float:
    """Writes one Cirq operation, returning the phase it leaves behind."""
    import cirq

    gate = operation.gate
    qubits = [index_of[qubit] for qubit in operation.qubits]

    if isinstance(gate, cirq.MeasurementGate):
        if gate.full_invert_mask() != (False,) * len(operation.qubits):
            raise ConversionError(
                f"measurement '{gate.key}' inverts some of its results, which "
                "this circuit cannot record"
            )
        start = clbit_of[gate.key]
        for offset, qubit in enumerate(qubits):
            target.measure(qubit, start + offset)
        return 0.0

    if isinstance(gate, cirq.ResetChannel):
        target.reset(qubits[0])
        return 0.0

    name = str(gate)
    if name in _DIRECT:
        getattr(target, _DIRECT[name])(*qubits)
        return 0.0

    # Matched by type rather than by name, because cirq.rx and friends are
    # subclasses that report their own name while behaving as the power gate.
    if len(qubits) == 1 and isinstance(gate, cirq.EigenGate):
        for kind, method in (
            (cirq.XPowGate, "rx"),
            (cirq.YPowGate, "ry"),
            (cirq.ZPowGate, "rz"),
        ):
            if isinstance(gate, kind):
                turn = float(gate.exponent)
                shift = float(gate.global_shift)
                getattr(target, method)(_HALF_TURN * turn, qubits[0])
                return _HALF_TURN * turn * (shift + 0.5)

    if len(qubits) == 2 and isinstance(gate, (cirq.CZPowGate, cirq.CXPowGate)):
        turn = float(gate.exponent)
        shift = float(gate.global_shift)
        angle = _HALF_TURN * turn
        # A controlled power of Z is a controlled phase, which is a controlled
        # rotation together with a rotation on the control. The controlled form
        # of X is the same thing with the target carried into the X basis.
        conjugate = isinstance(gate, cirq.CXPowGate)
        if conjugate:
            target.h(qubits[1])
        target.crz(angle, qubits[0], qubits[1])
        target.rz(angle / 2, qubits[0])
        if conjugate:
            target.h(qubits[1])
        return angle / 4 + angle * shift

    return _decompose(target, operation, index_of, clbit_of, depth)


def _decompose(
    target: QuantumCircuit,
    operation: Any,
    index_of: Dict[Any, int],
    clbit_of: Dict[str, int],
    depth: int,
) -> float:
    """Rewrites an operation through the decomposition Cirq gives for it.

    Only reached for operations with no direct counterpart, so that a gate this
    circuit can hold is never expanded into something smaller. Cirq's full
    decomposition would rewrite even H and CNOT into its own native set.
    """
    import cirq

    if depth >= _MAX_DEPTH:
        raise ConversionError(
            f"'{operation.gate}' is still not expressible after {_MAX_DEPTH} rewrites"
        )
    try:
        parts = cirq.decompose_once(operation)
    except (TypeError, ValueError) as error:
        raise ConversionError(
            f"'{operation.gate}' has no counterpart and cannot be rewritten"
        ) from error

    phase = 0.0
    for part in parts:
        if isinstance(part, cirq.GlobalPhaseGate):
            phase += _phase_of(part)
            continue
        inner = getattr(part, "gate", None)
        if isinstance(inner, cirq.GlobalPhaseGate):
            phase += _phase_of(inner)
            continue
        phase += _emit(target, part, index_of, clbit_of, depth + 1)
    return phase


def _phase_of(gate: Any) -> float:
    """The angle of a global phase gate."""
    import cmath

    return cmath.phase(complex(gate.coefficient))


# Our gates that Cirq names differently, or reaches through a different call.
_TO_CIRQ: Dict[str, str] = {
    "i": "I",
    "x": "X",
    "y": "Y",
    "z": "Z",
    "h": "H",
    "s": "S",
    "t": "T",
    "cx": "CNOT",
    "cy": "Y",
    "cz": "CZ",
    "swap": "SWAP",
    "ccx": "TOFFOLI",
    "cswap": "CSWAP",
}

_ROTATIONS_OUT = {"rx": "rx", "ry": "ry", "rz": "rz"}


def to_cirq(
    source: QuantumCircuit, qubits: Optional[Sequence[Any]] = None
) -> "cirq.Circuit":
    """Builds a Cirq circuit from ours.

    ``qubits`` supplies the objects to use, in index order, which is how the
    identities of a circuit that came from Cirq are restored. Without it the
    qubits are ``LineQubit`` in index order.

    Raises:
        ConversionError: when the source holds something Cirq cannot express.
    """
    import cirq

    if qubits is None:
        wires: Sequence[Any] = cirq.LineQubit.range(source.num_qubits)
    else:
        wires = list(qubits)
        if len(wires) != source.num_qubits:
            raise ConversionError(
                f"{len(wires)} qubits were supplied for a circuit of "
                f"{source.num_qubits}"
            )

    keys = _keys_by_clbit(source)
    built = cirq.Circuit()
    _write_all(
        built,
        [source[position] for position in range(len(source))],
        source,
        wires,
        keys,
    )
    if source.global_phase != 0.0:
        built.append(cirq.global_phase_operation(cmath.exp(1j * source.global_phase)))
    return built


def _keys_by_clbit(source: QuantumCircuit) -> Dict[int, Tuple[str, int]]:
    """Maps each classical bit to the measurement key it belongs to."""
    keys: Dict[int, Tuple[str, int]] = {}
    for register in source.classical_registers:
        for offset in range(register.size):
            keys[register.start + offset] = (register.name, offset)
    return keys


def _write_all(
    built: "cirq.Circuit",
    views: List[Any],
    source: QuantumCircuit,
    wires: Sequence[Any],
    keys: Dict[int, Tuple[str, int]],
) -> None:
    """Writes a run of instructions, regrouping what Cirq holds as one gate.

    A Cirq measurement can cover several qubits under one key, which this
    circuit stores as one measurement per qubit. Writing them back one at a
    time would produce several measurements sharing a key, which is a different
    circuit, so a run over one register is gathered back into a single gate.
    """
    import cirq

    position = 0
    while position < len(views):
        view = views[position]
        if view.name == "measure_z" and not view.blocks:
            key, offset = keys.get(view.clbits[0], ("", -1))
            if offset == 0:
                run = [view]
                while position + len(run) < len(views):
                    following = views[position + len(run)]
                    if following.name != "measure_z" or following.blocks:
                        break
                    next_key, next_offset = keys.get(following.clbits[0], ("", -1))
                    if next_key != key or next_offset != len(run):
                        break
                    run.append(following)
                if len(run) > 1:
                    built.append(
                        cirq.measure(
                            *[wires[member.qubits[0]] for member in run], key=key
                        )
                    )
                    position += len(run)
                    continue
        _write(built, view, source, wires, keys)
        position += 1


def _write(
    built: "cirq.Circuit",
    view: Any,
    source: QuantumCircuit,
    wires: Sequence[Any],
    keys: Dict[int, Tuple[str, int]],
) -> None:
    """Writes one of our instructions into a Cirq circuit."""
    import cirq

    name: str = view.name
    if view.blocks:
        if name == "moment":
            staged = cirq.Circuit()
            _write_all(staged, source.block(view.blocks[0]), source, wires, keys)
            built.append(
                cirq.Moment(op for moment in staged for op in moment.operations)
            )
            return
        raise ConversionError(f"control-flow construct '{name}' has no Cirq form")

    targets = [wires[index] for index in view.qubits]
    params = list(view.params)
    for value in params:
        if not isinstance(value, float):
            raise ConversionError(
                f"gate '{name}' carries a symbolic angle, which has to be bound "
                "before conversion"
            )

    if name == "measure_z":
        key, _ = keys.get(view.clbits[0], (f"c{view.clbits[0]}", 0))
        built.append(cirq.measure(*targets, key=key))
        return
    if name == "reset":
        built.append(cirq.reset(targets[0]))
        return
    if name in _ROTATIONS_OUT:
        built.append(getattr(cirq, name)(params[0]).on(targets[0]))
        return
    if name == "sdg":
        built.append((cirq.S**-1).on(targets[0]))
        return
    if name == "tdg":
        built.append((cirq.T**-1).on(targets[0]))
        return
    if name == "sx":
        built.append((cirq.X**0.5).on(targets[0]))
        return
    if name == "sxdg":
        built.append((cirq.X**-0.5).on(targets[0]))
        return
    if name == "cy":
        built.append(cirq.Y.controlled().on(*targets))
        return
    if name in ("crx", "cry", "crz"):
        rotation = getattr(cirq, name[1:])(params[0])
        built.append(rotation.controlled().on(*targets))
        return
    if name in ("rxx", "ryy", "rzz"):
        # A shift of minus a half turns Cirq's power gate into the rotation
        # form, which is what these gates mean.
        kind = {"rxx": cirq.XXPowGate, "ryy": cirq.YYPowGate, "rzz": cirq.ZZPowGate}[
            name
        ]
        built.append(
            kind(exponent=params[0] / _HALF_TURN, global_shift=-0.5).on(*targets)
        )
        return
    if name in ("barrier", "delay"):
        raise ConversionError(f"operation '{name}' has no Cirq form")

    gate = _TO_CIRQ.get(name)
    if gate is None:
        raise ConversionError(f"operation '{name}' has no Cirq counterpart")
    built.append(getattr(cirq, gate).on(*targets))
