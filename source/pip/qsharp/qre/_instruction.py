# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from abc import ABC, abstractmethod
from typing import Generator, Iterable, Optional, overload, cast
from enum import IntEnum

from ._enumeration import _enumerate_instances
from ._isa_enumeration import ISA_ROOT, BindingNode, ISAQuery, Node
from ._qre import (
    ISA,
    Constraint,
    ConstraintBound,
    FloatFunction,
    Instruction,
    IntFunction,
    ISARequirements,
    constant_function,
)


class Encoding(IntEnum):
    PHYSICAL = 0
    LOGICAL = 1


PHYSICAL = Encoding.PHYSICAL
LOGICAL = Encoding.LOGICAL


def constraint(
    id: int,
    encoding: Encoding = PHYSICAL,
    *,
    arity: Optional[int] = 1,
    error_rate: Optional[ConstraintBound] = None,
) -> Constraint:
    """
    Creates an instruction constraint.

    Args:
        id (int): The instruction ID.
        encoding (Encoding): The instruction encoding. PHYSICAL (0) or LOGICAL (1).
        arity (Optional[int]): The instruction arity. If None, instruction is
            assumed to have variable arity.  Default is 1.
        error_rate (Optional[ConstraintBound]): The constraint on the error rate.

    Returns:
        Constraint: The instruction constraint.
    """
    return Constraint(id, encoding, arity, error_rate)


@overload
def instruction(
    id: int,
    encoding: Encoding = PHYSICAL,
    *,
    time: int,
    arity: int = 1,
    space: Optional[int] = None,
    length: Optional[int] = None,
    error_rate: float,
) -> Instruction: ...
@overload
def instruction(
    id: int,
    encoding: Encoding = PHYSICAL,
    *,
    time: int | IntFunction,
    arity: None = ...,
    space: Optional[IntFunction] = None,
    length: Optional[IntFunction] = None,
    error_rate: FloatFunction,
) -> Instruction: ...
def instruction(
    id: int,
    encoding: Encoding = PHYSICAL,
    *,
    time: int | IntFunction,
    arity: Optional[int] = 1,
    space: Optional[int] | IntFunction = None,
    length: Optional[int | IntFunction] = None,
    error_rate: float | FloatFunction,
) -> Instruction:
    """
    Creates an instruction.

    Args:
        id (int): The instruction ID.
        encoding (Encoding): The instruction encoding. PHYSICAL (0) or LOGICAL (1).
        time (int | IntFunction): The instruction time in ns.
        arity (Optional[int]): The instruction arity.  If None, instruction is
            assumed to have variable arity.  Default is 1.  One can use variable arity
            functions for time, space, length, and error_rate in this case.
        space (Optional[int] | IntFunction): The instruction space in number of
            physical qubits. If None, length is used.
        length (Optional[int | IntFunction]): The arity including ancilla
            qubits. If None, arity is used.
        error_rate (float | FloatFunction): The instruction error rate.

    Returns:
        Instruction: The instruction.
    """
    if arity is not None:
        return Instruction.fixed_arity(
            id,
            encoding,
            arity,
            cast(int, time),
            cast(int | None, space),
            cast(int | None, length),
            cast(float, error_rate),
        )
    else:
        if isinstance(time, int):
            time = constant_function(time)
        if isinstance(space, int):
            space = constant_function(space)
        if isinstance(length, int):
            length = constant_function(length)
        if isinstance(error_rate, float):
            error_rate = constant_function(error_rate)

        return Instruction.variable_arity(
            id,
            encoding,
            time,
            cast(IntFunction, space),
            cast(FloatFunction, error_rate),
            length,
        )


class ISATransform(ABC):
    @staticmethod
    @abstractmethod
    def required_isa() -> ISARequirements: ...

    @abstractmethod
    def provided_isa(self, impl_isa: ISA) -> Generator[ISA, None, None]: ...

    @classmethod
    def enumerate_isas(
        cls,
        impl_isa: ISA | Iterable[ISA],
        **kwargs,
    ) -> Generator[ISA, None, None]:
        isas = [impl_isa] if isinstance(impl_isa, ISA) else impl_isa
        for isa in isas:
            if not isa.satisfies(cls.required_isa()):
                continue

            for component in _enumerate_instances(cls, **kwargs):
                yield from component.provided_isa(isa)

    @classmethod
    def q(cls, *, source: Node | None = None, **kwargs) -> ISAQuery:
        return ISAQuery(
            cls, source=source if source is not None else ISA_ROOT, kwargs=kwargs
        )

    @classmethod
    def bind(cls, name: str, node: Node) -> BindingNode:
        return cls.q().bind(name, node)
