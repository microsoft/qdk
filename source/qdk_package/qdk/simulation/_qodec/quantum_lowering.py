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
            operations.append(Operation("s_adj", (target,), noiseless=True))
        if operator[target] in ("X", "Y"):
            operations.append(Operation("h", (target,), noiseless=True))
    if operator.support:
        pivot = operator.support[-1]
        operations.extend(
            Operation("cx", (target, pivot), noiseless=True)
            for target in operator.support[:-1]
        )
    return tuple(operations)


def _adjoint(operations: Sequence[Operation]) -> tuple[Operation, ...]:
    return tuple(
        Operation(
            "s" if operation.name == "s_adj" else operation.name,
            operation.targets,
            noiseless=True,
        )
        for operation in reversed(operations)
    )


def _undo(operations: Sequence[Operation]) -> Requests[None]:
    for operation in _adjoint(operations):
        yield operation


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
                Operation(
                    "y" if x_bit and z_bit else "x" if x_bit else "z",
                    (target,),
                    noiseless=True,
                )
            )
    return tuple(operations)


_Bits = dict[int, tuple[bool, bool]]


def _pauli_bits(operator: DensePauli) -> _Bits:
    return {
        target: (operator[target] in ("X", "Y"), operator[target] in ("Y", "Z"))
        for target in operator.support
    }


def _correction_bits(correction: Sequence[Operation]) -> _Bits:
    return {
        local_indices(operation)[0]: (
            operation.name in ("x", "y"),
            operation.name in ("y", "z"),
        )
        for operation in correction
    }


def _conjugate(bits: _Bits, basis: Sequence[Operation]) -> _Bits:
    """The (X, Z) bits of a Pauli after the basis change, up to sign."""
    bits = dict(bits)
    for operation in basis:
        if operation.name == "cx":
            control, target = local_indices(operation)
            control_x, control_z = bits.get(control, (False, False))
            target_x, target_z = bits.get(target, (False, False))
            bits[control] = (control_x, control_z != target_z)
            bits[target] = (target_x != control_x, target_z)
            continue
        (target,) = local_indices(operation)
        x_bit, z_bit = bits.get(target, (False, False))
        if operation.name == "h":
            bits[target] = (z_bit, x_bit)
        elif operation.name in ("s", "s_adj"):
            bits[target] = (x_bit, z_bit != x_bit)
        else:
            raise ValueError(f"Unexpected basis change {operation.name!r}")
    return bits


def static_observation(operator: DensePauli) -> tuple[Operation, ...]:
    """Measure a Hermitian Pauli with one Z measurement between noiseless gates.

    A basis change maps the Pauli to Z on its last qubit, which is then measured;
    a negative sign flips that qubit around the measurement.
    """
    if not operator.support or operator.phase not in (1, -1):
        raise ValueError("A static observation requires a nonidentity Hermitian Pauli")
    basis = _basis(operator)
    pivot = operator.support[-1]
    sign = (Operation("x", (pivot,), noiseless=True),) if operator.phase == -1 else ()
    return (
        *basis,
        *sign,
        Operation("measure", (pivot,)),
        *sign,
        *_adjoint(basis),
    )


def static_stabilization(operators: Sequence[DensePauli]) -> tuple[Operation, ...]:
    """Prepare each Pauli's +1 eigenspace without branching on an outcome.

    Measuring a Pauli and applying a recovery when its outcome is -1 equals, as a
    channel, the following fixed sequence: map the Pauli to Z on its last qubit,
    apply the rest of the recovery controlled on that qubit, and reset it. Every
    gate is noiseless, so only the reset samples noise.
    """
    operations: list[Operation] = []
    previous: list[DensePauli] = []
    for operator in operators:
        if not operator.support or operator.phase not in (1, -1):
            raise ValueError(
                "A static stabilization requires nonidentity Hermitian Paulis"
            )
        basis = _basis(operator)
        pivot = operator.support[-1]
        # X on the pivot recovers unless it would disturb an earlier stabilizer.
        correction: _Bits = {pivot: (True, False)}
        if any(
            constraint.commutes_with(operator)
            and _conjugate(_pauli_bits(constraint), basis).get(pivot, (False, False))[1]
            for constraint in previous
        ):
            correction = _conjugate(
                _correction_bits(_recovery(operator, previous)), basis
            )
        if not correction.get(pivot, (False, False))[0]:
            raise AssertionError("A recovery must anticommute with its Pauli")
        controlled = [
            Operation(
                {(True, False): "cx", (False, True): "cz", (True, True): "cy"}[bits],
                (pivot, target),
                noiseless=True,
            )
            for target, bits in sorted(correction.items())
            if target != pivot and any(bits)
        ]
        # A negative sign makes the pivot's |0> the outcome that needs recovery.
        sign = (
            [Operation("x", (pivot,), noiseless=True)] if operator.phase == -1 else []
        )
        operations += [*basis, *sign, *controlled, Operation("prepare", (pivot,))]
        operations += [*sign, *_adjoint(basis)]
        previous.append(operator)
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
                    yield Operation("x", (target,), noiseless=True)
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
                        operation.noiseless,
                    )
