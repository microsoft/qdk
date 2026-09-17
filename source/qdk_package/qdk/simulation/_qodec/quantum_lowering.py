from collections.abc import Sequence
from contextlib import closing

from paulimer import DensePauli

from .action_runtime import ActionProgram
from .clifford_semantics import lower_clifford
from .protocols import ExecutionUnresolved, Readouts, Requests
from .quantum_instruments import (
    Allocate,
    CliffordGate,
    Instrument,
    Observation,
    PauliGate,
    PauliRotation,
    Stabilization,
    TraceOut,
)
from .quantum_operations import Operation, local_indices
from .readout_equations import BinarySystem, InconsistentParity, Parity


def _basis(operator: DensePauli) -> tuple[Operation, ...]:
    operations = []
    for target in operator.support:
        if operator[target] == "Y":
            operations.append(Operation("s_adj", (target,)))
        if operator[target] in ("X", "Y"):
            operations.append(Operation("h", (target,)))
    if operator.support:
        pivot = operator.support[-1]
        operations.extend(
            Operation("cx", (target, pivot)) for target in operator.support[:-1]
        )
    return tuple(operations)


def _undo(operations: Sequence[Operation]) -> Requests[None]:
    for operation in reversed(operations):
        yield Operation(
            "s" if operation.name == "s_adj" else operation.name, operation.targets
        )


def _commutation(operator: DensePauli, constant: bool = False) -> Parity:
    return Parity(
        frozenset(
            2 * target + component
            for target in operator.support
            for component in (0, 1)
            if operator[target] in (("Z", "Y") if component == 0 else ("X", "Y"))
        ),
        constant,
    )


def _recovery(
    operator: DensePauli, previous: Sequence[DensePauli]
) -> tuple[Operation, ...]:
    system = BinarySystem([_commutation(operator, True)])
    for constraint in previous:
        if constraint.commutes_with(operator):
            try:
                system.add(_commutation(constraint))
            except InconsistentParity:
                continue
    values = system.solution()
    operations = []
    for target in range(operator.size):
        x_bit, z_bit = values.get(2 * target, False), values.get(2 * target + 1, False)
        if x_bit or z_bit:
            operations.append(
                Operation("y" if x_bit and z_bit else "x" if x_bit else "z", (target,))
            )
    return tuple(operations)


def lower_instrument(instrument: Instrument) -> Requests[Readouts]:
    if isinstance(instrument, Allocate):
        for target in instrument.targets:
            yield Operation("prepare", (target,))
            yield Operation("h", (target,))
            yield Operation("measure", (target,))
    elif isinstance(instrument, TraceOut):
        for target in instrument.targets:
            yield Operation("discard", (target,))
    elif isinstance(instrument, CliffordGate):
        for operation in lower_clifford(instrument.operator):
            yield operation
    elif isinstance(instrument, PauliGate):
        for target in instrument.operator.support:
            yield Operation(instrument.operator[target].lower(), (target,))
    elif isinstance(instrument, (PauliRotation, Observation)):
        operator = instrument.operator
        if operator.phase not in (1, -1):
            raise ValueError("Quantum instruments require Hermitian Pauli operators")
        if not operator.support:
            return (
                (operator.phase == -1,) if isinstance(instrument, Observation) else ()
            )
        basis = _basis(operator)
        for operation in basis:
            yield operation
        pivot = operator.support[-1]
        readouts: Readouts = ()
        if isinstance(instrument, PauliRotation):
            yield Operation(
                "rz", (pivot,), instrument.angle * (1 if operator.phase == 1 else -1)
            )
        else:
            reply = yield Operation("measure", (pivot,))
            if reply is None or len(reply) != 1:
                raise ValueError("Pauli observation requires one measurement reply")
            value = reply[0]
            readouts = (None if value is None else value ^ (operator.phase == -1),)
        yield from _undo(basis)
        return readouts
    elif isinstance(instrument, Stabilization):
        previous = []
        for operator in instrument.operators:
            if not operator.support and operator.phase == -1:
                raise ValueError("The negative identity has no positive eigenspace")
            if (
                len(instrument.operators) == 1
                and len(operator.support) == 1
                and operator[operator.support[0]] == "Z"
            ):
                target = operator.support[0]
                yield Operation("prepare", (target,))
                if operator.phase == -1:
                    yield Operation("x", (target,))
            else:
                (value,) = yield from lower_instrument(Observation(operator))
                if value is None:
                    raise ExecutionUnresolved(
                        "Stabilization requires an unavailable observation"
                    )
                if value:
                    for operation in _recovery(operator, previous):
                        yield operation
            previous.append(operator)
    else:
        raise TypeError(f"Unknown quantum instrument {type(instrument).__name__}")
    return ()


def lower_program(program: ActionProgram, qubits: Sequence[int]) -> Requests[Readouts]:
    with closing(program.run()) as actions:
        reply = None
        while True:
            try:
                instrument = actions.send(reply)
            except StopIteration as completed:
                return completed.value
            with closing(lower_instrument(instrument)) as operations:
                reply = None
                while True:
                    try:
                        operation = operations.send(reply)
                    except StopIteration as completed:
                        reply = completed.value
                        break
                    if not isinstance(operation, Operation):
                        raise TypeError(
                            "Instrument lowering must produce primitive operations"
                        )
                    reply = yield Operation(
                        operation.name,
                        tuple(qubits[index] for index in local_indices(operation)),
                        operation.angle,
                    )
