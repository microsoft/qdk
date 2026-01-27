# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from typing import Optional, overload
from enum import IntEnum

from ._qre import (
    Instruction,
    Constraint,
    FloatFunction,
    IntFunction,
    constant_function,
    ConstraintBound,
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
    error_rate: Optional[ConstraintBound] = None
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
    error_rate: Optional[float] = None
) -> Instruction: ...
@overload
def instruction(
    id: int,
    encoding: Encoding = PHYSICAL,
    *,
    time: IntFunction,
    arity: None = ...,
    space: Optional[IntFunction] = None,
    length: Optional[IntFunction] = None,
    error_rate: Optional[FloatFunction] = None
) -> Instruction: ...
def instruction(
    id: int,
    encoding: Encoding = PHYSICAL,
    *,
    time: int | IntFunction,
    arity: Optional[int] = 1,
    space: Optional[int] | IntFunction = None,
    length: Optional[int | IntFunction] = None,
    error_rate: float | FloatFunction = None
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
            id, encoding, arity, time, space, length, error_rate
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

        return Instruction.variable_arity(id, encoding, time, space, error_rate, length)
