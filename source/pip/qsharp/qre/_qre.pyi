# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations
from typing import Iterator, Optional, overload

class ISA:
    @overload
    def __new__(cls, *instructions: Instruction) -> ISA: ...
    @overload
    def __new__(cls, instructions: list[Instruction], /) -> ISA: ...
    def __new__(cls, *instructions: Instruction | list[Instruction]) -> ISA:
        """
        Creates an ISA from a list of instructions.

        Args:
            instructions (list[Instruction] | *Instruction): The list of instructions.
        """
        ...

    def __add__(self, other: ISA) -> ISA:
        """
        Concatenates two ISAs (logical union). Instructions in the second
        operand overwrite instructions in the first operand if they have the
        same ID.
        """
        ...

    def satisfies(self, requirements: ISARequirements) -> bool:
        """
        Checks if the ISA satisfies the given ISA requirements.
        """
        ...

    def __getitem__(self, id: int) -> Instruction:
        """
        Gets an instruction by its ID.

        Args:
            id (int): The instruction ID.

        Returns:
            Instruction: The instruction.
        """
        ...

    def __len__(self) -> int:
        """
        Returns the number of instructions in the ISA.

        Returns:
            int: The number of instructions.
        """
        ...

    def __iter__(self) -> Iterator[Instruction]:
        """
        Returns an iterator over the instructions.

        Note:
            The order of instructions is not guaranteed.

        Returns:
            Iterator[Instruction]: The instruction iterator.
        """
        ...

    def __str__(self) -> str:
        """
        Returns a string representation of the ISA.

        Note:
            The order of instructions in the output is not guaranteed.

        Returns:
            str: A string representation of the ISA.
        """
        ...

class ISARequirements:
    @overload
    def __new__(cls, *constraints: Constraint) -> ISARequirements: ...
    @overload
    def __new__(cls, constraints: list[Constraint], /) -> ISARequirements: ...
    def __new__(cls, *constraints: Constraint | list[Constraint]) -> ISARequirements:
        """
        Creates an ISA requirements specification from a list of instructions
        constraints.

        Args:
            constraints (list[InstructionConstraint] | *InstructionConstraint): The list of instruction
                constraints.
        """
        ...

class Instruction:
    @staticmethod
    def fixed_arity(
        id: int,
        encoding: int,
        arity: int,
        time: int,
        space: Optional[int],
        length: Optional[int],
        error_rate: float,
    ) -> Instruction:
        """
        Creates an instruction with a fixed arity.

        Note:
            This function is not intended to be called directly by the user, use qre.instruction instead.

        Args:
            id (int): The instruction ID.
            encoding (int): The instruction encoding. 0 = Physical, 1 = Logical.
            arity (int): The instruction arity.
            time (int): The instruction time in ns.
            space (Optional[int]): The instruction space in number of physical
                qubits.  If None, length is used.
            length (Optional[int]): The arity including ancilla qubits.  If None,
                arity is used.
            error_rate (float): The instruction error rate.

        Returns:
            Instruction: The instruction.
        """
        ...

    @staticmethod
    def variable_arity(
        id: int,
        encoding: int,
        time_fn: IntFunction,
        space_fn: IntFunction,
        error_rate_fn: FloatFunction,
        length_fn: Optional[IntFunction],
    ) -> Instruction:
        """
        Creates an instruction with variable arity.

        Note:
            This function is not intended to be called directly by the user, use qre.instruction instead.

        Args:
            id (int): The instruction ID.
            encoding (int): The instruction encoding. 0 = Physical, 1 = Logical.
            time_fn (IntFunction): The time function.
            space_fn (IntFunction): The space function.
            error_rate_fn (FloatFunction): The error rate function.
            length_fn (Optional[IntFunction]): The length function.
                If None, space_fn is used.

        Returns:
            Instruction: The instruction.
        """
        ...

    @property
    def id(self) -> int:
        """
        The instruction ID.

        Returns:
            int: The instruction ID.
        """
        ...

    @property
    def encoding(self) -> int:
        """
        The instruction encoding. 0 = Physical, 1 = Logical.

        Returns:
            int: The instruction encoding.
        """
        ...

    @property
    def arity(self) -> Optional[int]:
        """
        The instruction arity.

        Returns:
            Optional[int]: The instruction arity.
        """
        ...

    def space(self, arity: Optional[int] = None) -> Optional[int]:
        """
        The instruction space in number of physical qubits.

        Args:
            arity (Optional[int]): The specific arity to check.

        Returns:
            Optional[int]: The instruction space in number of physical qubits.
        """
        ...

    def time(self, arity: Optional[int] = None) -> Optional[int]:
        """
        The instruction time in ns.

        Args:
            arity (Optional[int]): The specific arity to check.

        Returns:
            Optional[int]: The instruction time in ns.
        """
        ...

    def error_rate(self, arity: Optional[int] = None) -> Optional[float]:
        """
        The instruction error rate.

        Args:
            arity (Optional[int]): The specific arity to check.

        Returns:
            Optional[float]: The instruction error rate.
        """
        ...

    def expect_space(self, arity: Optional[int] = None) -> int:
        """
        The instruction space in number of physical qubits. Raises an error if not found.

        Args:
            arity (Optional[int]): The specific arity to check.

        Returns:
            int: The instruction space in number of physical qubits.
        """
        ...

    def expect_time(self, arity: Optional[int] = None) -> int:
        """
        The instruction time in ns. Raises an error if not found.

        Args:
            arity (Optional[int]): The specific arity to check.

        Returns:
            int: The instruction time in ns.
        """
        ...

    def expect_error_rate(self, arity: Optional[int] = None) -> float:
        """
        The instruction error rate. Raises an error if not found.

        Args:
            arity (Optional[int]): The specific arity to check.

        Returns:
            float: The instruction error rate.
        """
        ...

    def __str__(self) -> str:
        """
        Returns a string representation of the instruction.

        Returns:
            str: A string representation of the instruction.
        """
        ...

class ConstraintBound:
    """
    A bound for a constraint.
    """

    @staticmethod
    def lt(value: float) -> ConstraintBound:
        """
        Creates a less than constraint bound.

        Args:
            value (float): The value.

        Returns:
            ConstraintBound: The constraint bound.
        """
        ...

    @staticmethod
    def le(value: float) -> ConstraintBound:
        """
        Creates a less equal constraint bound.

        Args:
            value (float): The value.

        Returns:
            ConstraintBound: The constraint bound.
        """
        ...

    @staticmethod
    def eq(value: float) -> ConstraintBound:
        """
        Creates an equal constraint bound.

        Args:
            value (float): The value.

        Returns:
            ConstraintBound: The constraint bound.
        """
        ...

    @staticmethod
    def gt(value: float) -> ConstraintBound:
        """
        Creates a greater than constraint bound.

        Args:
            value (float): The value.

        Returns:
            ConstraintBound: The constraint bound.
        """
        ...

    @staticmethod
    def ge(value: float) -> ConstraintBound:
        """
        Creates a greater equal constraint bound.

        Args:
            value (float): The value.

        Returns:
            ConstraintBound: The constraint bound.
        """
        ...

class Constraint:
    """
    An instruction constraint that can be used to describe ISA requirements
    for ISA transformations.
    """

    def __new__(
        cls,
        id: int,
        encoding: int,
        arity: Optional[int],
        error_rate: Optional[ConstraintBound],
    ) -> Constraint:
        """
        Note:
            This function is not intended to be called directly by the user, use qre.constraint instead.

        Args:
            id (int): The instruction ID.
            encoding (int): The instruction encoding. 0 = Physical, 1 = Logical.
            arity (Optional[int]): The instruction arity. If None, instruction is
                assumed to have variable arity.
            error_rate (Optional[ConstraintBound]): The constraint on the error rate.

        Returns:
            InstructionConstraint: The instruction constraint.
        """
        ...

class IntFunction: ...
class FloatFunction: ...

@overload
def constant_function(value: int) -> IntFunction: ...
@overload
def constant_function(value: float) -> FloatFunction: ...
def constant_function(
    value: int | float,
) -> IntFunction | FloatFunction:
    """
    Creates a constant function.

    Args:
        value (int | float): The constant value.

    Returns:
        IntFunction | FloatFunction: The constant function.
    """
    ...

@overload
def linear_function(slope: int) -> IntFunction: ...
@overload
def linear_function(slope: float) -> FloatFunction: ...
def linear_function(
    slope: int | float,
) -> IntFunction | FloatFunction:
    """
    Creates a linear function.

    Args:
        slope (int | float): The slope.

    Returns:
        IntFunction | FloatFunction: The linear function.
    """
    ...

@overload
def block_linear_function(block_size: int, slope: int) -> IntFunction: ...
@overload
def block_linear_function(block_size: int, slope: float) -> FloatFunction: ...
def block_linear_function(
    block_size: int, slope: int | float
) -> IntFunction | FloatFunction:
    """
    Creates a block linear function.

    Args:
        block_size (int): The block size.
        slope (int | float): The slope.

    Returns:
        IntFunction | FloatFunction: The block linear function.
    """
    ...
