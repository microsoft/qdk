from __future__ import annotations

from collections.abc import Generator, Mapping
from dataclasses import dataclass
import re

from paulimer import DensePauli, SparsePauli
from qodec import Instruction
from qodec import actions
from qodec.instructions import InstructionCall, Parameter

from .call_binding import validate_arguments
from .clifford_semantics import clifford_tableau, pauli
from .protocols import ExecutionUnresolved, Readouts
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


@dataclass(frozen=True)
class Guard:
    outcomes: tuple[int, ...]
    constant: bool
    expected: bool

    def accepts(self, readouts: Readouts) -> bool:
        parity = self.constant
        for index in self.outcomes:
            value = readouts[index]
            if value is None:
                raise ExecutionUnresolved(
                    "Action guard requires an unavailable outcome"
                )
            parity ^= value
        return parity == self.expected


@dataclass(frozen=True)
class ActionStep:
    instrument: Instrument
    guard: Guard | None = None


@dataclass(frozen=True)
class ActionProgram:
    steps: tuple[ActionStep, ...]
    num_qubits: int

    def run(self) -> Generator[Instrument, Readouts | None, Readouts]:
        outcomes: Readouts = ()
        for step in self.steps:
            if step.guard is not None and not step.guard.accepts(outcomes):
                continue
            reply = yield step.instrument
            count = 1 if isinstance(step.instrument, Observation) else 0
            if reply is None or len(reply) != count:
                raise ValueError(
                    f"Quantum instrument must return {count} reported outcomes"
                )
            outcomes += reply
        return outcomes


def _mapped(operator: DensePauli, indices: Mapping[int, int]) -> DensePauli:
    if operator.phase not in (1, -1):
        raise ValueError("Action Pauli operators must be Hermitian")
    if set(operator.support) - indices.keys():
        raise ValueError("Action uses a qubit before its preparation")
    terms = (
        " ".join(f"{operator[index]}{indices[index]}" for index in operator.support)
        or "I"
    )
    return DensePauli.from_sparse(
        SparsePauli(("-" if operator.phase == -1 else "") + terms), len(indices)
    )


def temporary_count(instruction: Instruction, boundary_capacity: int) -> int:
    parameters = {parameter.name for parameter in instruction.parameters}
    return len(
        {
            index
            for action in instruction.action
            if isinstance(action, actions.Stabilize)
            for value in action.operators
            if value not in parameters
            for index in pauli(value).support
            if index >= boundary_capacity
        }
    )


def prepare_actions(
    instruction: Instruction,
    input_capacity: int,
    output_capacity: int,
    arguments: Mapping[str, InstructionCall.Argument],
) -> ActionProgram:
    validate_arguments(instruction, arguments)
    indices = {index: index for index in range(max(input_capacity, output_capacity))}
    parameters = {
        parameter.name: parameter.kind for parameter in instruction.parameters
    }
    steps: list[ActionStep] = []
    if output_capacity > input_capacity:
        steps.append(
            ActionStep(Allocate(tuple(range(input_capacity, output_capacity))))
        )
    outcome_count = 0

    def operator(value: str) -> DensePauli:
        if value in parameters:
            argument = arguments[value]
            if parameters[value] != Parameter.Kind.PAULI or not isinstance(
                argument, str
            ):
                raise TypeError(
                    "Action Pauli expression requires a scalar Pauli parameter"
                )
            value = argument
        return pauli(value)

    for action in instruction.action:
        condition = getattr(action, "condition", None)
        guard = None
        if condition is not None:
            used = []
            constant = False
            for predicate in condition.predicates:
                outcome = re.fullmatch(r"outcomes\[(\d+)\]", predicate)
                if outcome is not None:
                    index = int(outcome.group(1))
                    if index >= outcome_count:
                        raise ValueError(
                            "Action guards can only use preceding outcomes"
                        )
                    if index in used:
                        used.remove(index)
                    else:
                        used.append(index)
                elif parameters.get(predicate) == Parameter.Kind.BIT:
                    value = arguments[predicate]
                    if not isinstance(value, (bool, int)):
                        raise TypeError("Action guards require scalar bit parameters")
                    constant ^= bool(value)
                else:
                    raise ValueError(
                        "Action guards require declared bit parameters or earlier outcomes"
                    )
            guard = Guard(tuple(used), constant, not condition.invert)
        if isinstance(action, actions.Stabilize):
            operators = tuple(operator(value) for value in action.operators)
            introduced = sorted(
                {index for item in operators for index in item.support} - indices.keys()
            )
            if introduced and condition is not None:
                raise ValueError(
                    "Conditional stabilization cannot introduce temporary qubits"
                )
            if introduced:
                allocated = tuple(range(len(indices), len(indices) + len(introduced)))
                indices.update(zip(introduced, allocated))
                steps.append(ActionStep(Allocate(allocated)))
            steps.append(
                ActionStep(
                    Stabilization(tuple(_mapped(item, indices) for item in operators)),
                    guard,
                )
            )
        elif isinstance(action, actions.Observe):
            if guard is not None:
                raise ValueError("Observations cannot be conditional")
            for value in action.observables:
                steps.append(ActionStep(Observation(_mapped(operator(value), indices))))
                outcome_count += 1
        elif isinstance(action, actions.Pauli):
            steps.append(
                ActionStep(
                    PauliGate(_mapped(operator(action.operator), indices)), guard
                )
            )
        elif isinstance(action, actions.Rotate):
            angle = (
                arguments[action.angle]
                if isinstance(action.angle, str)
                else action.angle
            )
            if not isinstance(angle, (int, float)) or isinstance(angle, bool):
                raise TypeError("Action rotation angle requires a scalar number")
            steps.append(
                ActionStep(
                    PauliRotation(
                        _mapped(operator(action.pauli), indices), float(angle)
                    ),
                    guard,
                )
            )
        elif isinstance(action, actions.Clifford):
            generators = {}
            for source, target in action.generators.items():
                generator = _mapped(pauli(source), indices)
                if (
                    generator.phase != 1
                    or len(generator.support) != 1
                    or generator[generator.support[0]] not in ("X", "Z")
                ):
                    raise ValueError(
                        "Clifford keys must be positive individual X or Z generators"
                    )
                if generator.characters in generators:
                    raise ValueError("Clifford keys must not alias the same generator")
                image = _mapped(pauli(target), indices)
                generators[generator.characters] = (
                    "-" if image.phase == -1 else ""
                ) + image.characters
            steps.append(
                ActionStep(
                    CliffordGate(clifford_tableau(generators, len(indices))), guard
                )
            )
        else:
            raise NotImplementedError(f"Unknown action type {type(action).__name__}")
    discarded = tuple(
        mapped for original, mapped in indices.items() if original >= output_capacity
    )
    if discarded:
        steps.append(ActionStep(TraceOut(discarded)))
    return ActionProgram(tuple(steps), len(indices))
