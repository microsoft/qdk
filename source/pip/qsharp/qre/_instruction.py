# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from typing import Optional

from ._qre import (
    Instruction,
    Constraint,
    FloatFunction,
    IntFunction,
    constant_function,
    ConstraintBound,
)

PHYSICAL = 0
LOGICAL = 1


LESS_THAN = 0
LESS_EQUAL = 1
EQUAL = 2
GREATER_THAN = 3
GREATER_EQUAL = 4


def constraint(
    id: int,
    encoding: int = PHYSICAL,
    *,
    arity: Optional[int] = 1,
    error_rate: Optional[ConstraintBound] = None
) -> Constraint:
    """
    Creates an instruction constraint.

    Args:
        id (int): The instruction ID.
        encoding (int): The instruction encoding. 0 = Physical (default), 1 = Logical.
        arity (Optional[int]): The instruction arity. If None, instruction is
            assumed to have variable arity.  Default is 1.
        error_rate (Optional[ConstraintBound]): The constraint on the error rate.

    Returns:
        Constraint: The instruction constraint.
    """
    return Constraint(id, encoding, arity, error_rate)


def instruction(
    id: int,
    encoding: int = PHYSICAL,
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
        encoding (int): The instruction encoding. 0 = Physical (default), 1 = Logical.
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
