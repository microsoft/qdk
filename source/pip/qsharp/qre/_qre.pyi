# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations
from typing import Any, Callable, Iterator, Optional, overload

class ISA:
    @overload
    def __new__(cls, *instructions: _Instruction) -> ISA: ...
    @overload
    def __new__(cls, instructions: list[_Instruction], /) -> ISA: ...
    def __new__(cls, *instructions: _Instruction | list[_Instruction]) -> ISA:
        """
        Creates an ISA from a list of instructions.

        Args:
            instructions (list[_Instruction] | *_Instruction): The list of instructions.
        """
        ...

    def append(self, instruction: _Instruction) -> None:
        """
        Appends an instruction to the ISA.

        Args:
            instruction (_Instruction): The instruction to append.
        """
        ...

    def __add__(self, other: ISA) -> ISA:
        """
        Concatenates two ISAs (logical union). Instructions in the second
        operand overwrite instructions in the first operand if they have the
        same ID.
        """
        ...

    def __contains__(self, id: int) -> bool:
        """
        Checks if the ISA contains an instruction with the given ID.

        Args:
            id (int): The instruction ID.

        Returns:
            bool: True if the ISA contains an instruction with the given ID, False otherwise.
        """
        ...

    def satisfies(self, requirements: ISARequirements) -> bool:
        """
        Checks if the ISA satisfies the given ISA requirements.
        """
        ...

    def __getitem__(self, id: int) -> _Instruction:
        """
        Gets an instruction by its ID.

        Args:
            id (int): The instruction ID.

        Returns:
            _Instruction: The instruction.
        """
        ...

    def get(
        self, id: int, default: Optional[_Instruction] = None
    ) -> Optional[_Instruction]:
        """
        Gets an instruction by its ID, or returns a default value if not found.

        Args:
            id (int): The instruction ID.
            default (Optional[_Instruction]): The default value to return if the
                instruction is not found.

        Returns:
            Optional[_Instruction]: The instruction, or the default value if not found.
        """
        ...

    def __len__(self) -> int:
        """
        Returns the number of instructions in the ISA.

        Returns:
            int: The number of instructions.
        """
        ...

    def __iter__(self) -> Iterator[_Instruction]:
        """
        Returns an iterator over the instructions.

        Note:
            The order of instructions is not guaranteed.

        Returns:
            Iterator[_Instruction]: The instruction iterator.
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

class _Instruction:
    @staticmethod
    def fixed_arity(
        id: int,
        encoding: int,
        arity: int,
        time: int,
        space: Optional[int],
        length: Optional[int],
        error_rate: float,
    ) -> _Instruction:
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
            _Instruction: The instruction.
        """
        ...

    @staticmethod
    def variable_arity(
        id: int,
        encoding: int,
        time_fn: _IntFunction,
        space_fn: _IntFunction,
        error_rate_fn: _FloatFunction,
        length_fn: Optional[_IntFunction],
    ) -> _Instruction:
        """
        Creates an instruction with variable arity.

        Note:
            This function is not intended to be called directly by the user, use qre.instruction instead.

        Args:
            id (int): The instruction ID.
            encoding (int): The instruction encoding. 0 = Physical, 1 = Logical.
            time_fn (_IntFunction): The time function.
            space_fn (_IntFunction): The space function.
            error_rate_fn (_FloatFunction): The error rate function.
            length_fn (Optional[_IntFunction]): The length function.
                If None, space_fn is used.

        Returns:
            _Instruction: The instruction.
        """
        ...

    def with_id(self, id: int) -> _Instruction:
        """
        Returns a copy of the instruction with the given ID.

        Note:
            The created instruction will not inherit the source property of the
            original instruction and must be set by the user if intended.

        Args:
            id (int): The instruction ID.

        Returns:
            _Instruction: A copy of the instruction with the given ID.
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

    def set_source(self, index: int) -> None:
        """
        Sets the source index for the instruction.

        Args:
            index (int): The source index to set.
        """
        ...

    @property
    def source(self) -> int:
        """
        Gets the source index for the instruction.

        Returns:
            int: The source index for the instruction.
        """
        ...

    def set_property(self, key: int, value: int) -> None:
        """
        Sets a property on the instruction.

        Args:
            key (int): The property key.
            value (int): The property value.
        """
        ...

    def get_property(self, key: int) -> Optional[int]:
        """
        Gets a property by its key.

        Args:
            key (int): The property key.

        Returns:
            Optional[int]: The property value, or None if not found.
        """
        ...

    def has_property(self, key: int) -> bool:
        """
        Checks if the instruction has a property with the given key.

        Args:
            key (int): The property key.

        Returns:
            bool: True if the instruction has the property, False otherwise.
        """
        ...

    def get_property_or(self, key: int, default: int) -> int:
        """
        Gets a property by its key, or returns a default value if not found.

        Args:
            key (int): The property key.
            default (int): The default value to return if the property is not found.

        Returns:
            int: The property value, or the default value if not found.
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

    def add_property(self, property: int) -> None:
        """
        Adds a property requirement to the constraint.

        Args:
            property (int): The property key that must be present in matching instructions.
        """
        ...

    def has_property(self, property: int) -> bool:
        """
        Checks if the constraint requires a specific property.

        Args:
            property (int): The property key to check.

        Returns:
            bool: True if the constraint requires this property, False otherwise.
        """
        ...

class _IntFunction: ...
class _FloatFunction: ...

@overload
def constant_function(value: int) -> _IntFunction: ...
@overload
def constant_function(value: float) -> _FloatFunction: ...
def constant_function(
    value: int | float,
) -> _IntFunction | _FloatFunction:
    """
    Creates a constant function.

    Args:
        value (int | float): The constant value.

    Returns:
        _IntFunction | _FloatFunction: The constant function.
    """
    ...

@overload
def linear_function(slope: int) -> _IntFunction: ...
@overload
def linear_function(slope: float) -> _FloatFunction: ...
def linear_function(
    slope: int | float,
) -> _IntFunction | _FloatFunction:
    """
    Creates a linear function.

    Args:
        slope (int | float): The slope.

    Returns:
        _IntFunction | _FloatFunction: The linear function.
    """
    ...

@overload
def block_linear_function(block_size: int, slope: int) -> _IntFunction: ...
@overload
def block_linear_function(block_size: int, slope: float) -> _FloatFunction: ...
def block_linear_function(
    block_size: int, slope: int | float
) -> _IntFunction | _FloatFunction:
    """
    Creates a block linear function.

    Args:
        block_size (int): The block size.
        slope (int | float): The slope.

    Returns:
        _IntFunction | _FloatFunction: The block linear function.
    """
    ...

@overload
def generic_function(func: Callable[[int], int]) -> _IntFunction: ...
@overload
def generic_function(func: Callable[[int], float]) -> _FloatFunction: ...
def generic_function(
    func: Callable[[int], int | float],
) -> _IntFunction | _FloatFunction:
    """
    Creates a generic function from a Python callable.

    Note:
        Only use this function if the other function constructors
        (constant_function, linear_function, and block_linear_function) do not
        meet your needs, as using a Python callable can have performance
        implications.  If using this function, keep the logic in the callable as
        simple as possible to minimize overhead.

    Args:
        func (Callable[[int], int | float]): The Python callable.

    Returns:
        _IntFunction | _FloatFunction: The generic function.
    """
    ...

class _ProvenanceGraph:
    """
    Represents the provenance graph of instructions in a trace.  Each node in
    the graph corresponds to an instruction and the transform from which it was
    produced, and edges represent transformations applied to instructions during
    enumeration.
    """

    def add_node(
        self, instruction_id: int, transform_id: int, children: list[int]
    ) -> int:
        """
        Adds a node to the provenance graph.

        Args:
            instruction_id (int): The instruction ID corresponding to the node.
            transform_id (int): The transform ID corresponding to the node.
            children (list[int]): The list of child node indices in the provenance graph.

        Returns:
            int: The index of the added node in the provenance graph.
        """
        ...

    def instruction_id(self, node_index: int) -> int:
        """
        Returns the instruction ID for a given node index.

        Args:
            node_index (int): The index of the node in the provenance graph.

        Returns:
            int: The instruction ID corresponding to the node.
        """
        ...

    def transform_id(self, node_index: int) -> int:
        """
        Returns the transform ID for a given node index.

        Args:
            node_index (int): The index of the node in the provenance graph.

        Returns:
            int: The transform ID corresponding to the node.
        """
        ...

    def children(self, node_index: int) -> list[int]:
        """
        Returns the list of child node indices for a given node index.

        Args:
            node_index (int): The index of the node in the provenance graph.

        Returns:
            list[int]: The list of child node indices.
        """
        ...

    def num_nodes(self) -> int:
        """
        Returns the number of nodes in the provenance graph.

        Returns:
            int: The number of nodes in the provenance graph.
        """
        ...

    def num_edges(self) -> int:
        """
        Returns the number of edges in the provenance graph.

        Returns:
            int: The number of edges in the provenance graph.
        """
        ...

class EstimationResult:
    """
    Represents the result of a resource estimation.
    """

    @property
    def qubits(self) -> int:
        """
        The number of logical qubits.

        Returns:
            int: The number of logical qubits.
        """
        ...

    @property
    def runtime(self) -> int:
        """
        The runtime in nanoseconds.

        Returns:
            int: The runtime in nanoseconds.
        """
        ...

    @property
    def error(self) -> float:
        """
        The error probability of the computation.

        Returns:
            float: The error probability of the computation.
        """
        ...

    @property
    def factories(self) -> dict[int, FactoryResult]:
        """
        The factory results.

        Returns:
            dict[int, FactoryResult]: A dictionary mapping factory IDs to their results.
        """
        ...

    @property
    def isa(self) -> ISA:
        """
        The ISA used for the estimation.

        Returns:
            ISA: The ISA used for the estimation.
        """
        ...

    @property
    def properties(self) -> dict[str, bool | int | float | str]:
        """
        Custom properties from application generation and trace transform.

        Returns:
            dict[str, bool | int | float | str]: A dictionary mapping property keys to their values.
        """
        ...

    def __str__(self) -> str:
        """
        Returns a string representation of the estimation result.

        Returns:
            str: A string representation of the estimation result.
        """
        ...

class _EstimationCollection:
    """
    Represents a collection of estimation results.  Results are stored as a 2D
    Pareto frontier with physical qubits and runtime as objectives.
    """

    def __new__(cls) -> _EstimationCollection:
        """
        Creates a new estimation collection.

        Returns:
            _EstimationCollection: The estimation collection.
        """
        ...

    def insert(self, result: EstimationResult) -> None:
        """
        Inserts an estimation result into the collection.

        Args:
            result (EstimationResult): The estimation result to insert.
        """
        ...

    def __len__(self) -> int:
        """
        Returns the number of estimation results in the collection.

        Returns:
            int: The number of estimation results.
        """
        ...

    def __iter__(self) -> Iterator[EstimationResult]:
        """
        Returns an iterator over the estimation results.

        Returns:
            Iterator[EstimationResult]: The estimation result iterator.
        """
        ...

class FactoryResult:
    """
    Represents the result of a factory used in resource estimation.
    """

    @property
    def copies(self) -> int:
        """
        The number of factory copies.

        Returns:
            int: The number of factory copies.
        """
        ...

    @property
    def runs(self) -> int:
        """
        The number of factory runs.

        Returns:
            int: The number of factory runs.
        """
        ...

    @property
    def error_rate(self) -> float:
        """
        The error rate of the factory.

        Returns:
            float: The error rate of the factory.
        """
        ...

    @property
    def states(self) -> int:
        """
        The number of states produced by the factory.

        Returns:
            int: The number of states produced by the factory.
        """
        ...

class Trace:
    """
    Represents a quantum program optimized for resource estimation.

    A trace originates from a quantum application and can be modified via trace
    transformations. It consists of blocks of operations.
    """

    def __new__(cls, compute_qubits: int) -> Trace:
        """
        Creates a new trace.

        Returns:
            Trace: The trace.
        """
        ...

    def clone_empty(self, compute_qubits: Optional[int] = None) -> Trace:
        """
        Creates a new trace with the same metadata but empty block.

        Args:
            compute_qubits (Optional[int]): The number of compute qubits. If None,
                the number of compute qubits of the original trace is used.

        Returns:
            Trace: The new trace.
        """
        ...

    @property
    def compute_qubits(self) -> int:
        """
        The number of compute qubits.

        Returns:
            int: The number of compute qubits.
        """
        ...

    @property
    def base_error(self) -> float:
        """
        The base error of the trace.

        Returns:
            float: The base error of the trace.
        """
        ...

    def increment_base_error(self, amount: float) -> None:
        """
        Increments the base error.

        Args:
            amount (float): The amount to increment.
        """
        ...

    @property
    def memory_qubits(self) -> Optional[int]:
        """
        The number of memory qubits, if set.

        Returns:
            Optional[int]: The number of memory qubits, or None if not set.
        """
        ...

    def has_memory_qubits(self) -> bool:
        """
        Checks if the trace has memory qubits set.

        Returns:
            bool: True if memory qubits are set, False otherwise.
        """
        ...

    def set_memory_qubits(self, qubits: int) -> None:
        """
        Sets the number of memory qubits.

        Args:
            qubits (int): The number of memory qubits.
        """
        ...

    def increment_memory_qubits(self, amount: int) -> None:
        """
        Increments the number of memory qubits. If memory qubits have not been
        set, initializes them to 0 before incrementing.

        Args:
            amount (int): The amount to increment.
        """
        ...

    def increment_resource_state(self, resource_id: int, amount: int) -> None:
        """
        Increments a resource state count.

        Args:
            resource_id (int): The resource state ID.
            amount (int): The amount to increment.
        """
        ...

    def set_property(self, key: str, value: Any) -> None:
        """
        Sets a property.  All values of type `int`, `float`, `bool`, and `str`
        are supported.  Any other value is converted to a string using its
        `__str__` method.

        Args:
            key (str): The property key.
            value (Any): The property value.
        """
        ...

    def get_property(self, key: str) -> Optional[int | float | bool | str]:
        """
        Gets a property.

        Args:
            key (str): The property key.

        Returns:
            Optional[int | float | bool | str]: The property value, or None if not found.
        """
        ...

    @property
    def depth(self) -> int:
        """
        The trace depth.

        Returns:
            int: The trace depth.
        """
        ...

    def estimate(
        self, isa: ISA, max_error: Optional[float] = None
    ) -> Optional[EstimationResult]:
        """
        Estimates resources for the trace given a logical ISA.

        Args:
            isa (ISA): The logical ISA.
            max_error (Optional[float]): The maximum allowed error. If None,
                Pareto points are computed.

        Returns:
            Optional[EstimationResult]: The estimation result if max_error is
                provided, otherwise valid Pareto points.
        """
        ...  # The implementation in Rust returns Option<EstimationResult>, so it fits

    @property
    def resource_states(self) -> dict[int, int]:
        """
        The resource states used in the trace.

        Returns:
            dict[int, int]: A dictionary mapping instruction IDs to their counts.
        """
        ...

    def add_operation(
        self, id: int, qubits: list[int], params: list[float] = []
    ) -> None:
        """
        Adds an operation to the trace.

        Args:
            id (int): The operation ID.
            qubits (list[int]): The qubits involved in the operation.
            params (list[float]): The operation parameters.
        """
        ...

    def add_block(self, repetitions: int = 1) -> Block:
        """
        Adds a block to the trace.

        Args:
            repetitions (int): The number of times the block is repeated.

        Returns:
            Block: The block.
        """
        ...

    def __str__(self) -> str:
        """
        Returns a string representation of the trace.

        Returns:
            str: A string representation of the trace.
        """
        ...

class Block:
    """
    Represents a block of operations in a trace.

    An operation in a block can either refer to an instruction applied to some
    qubits or can be another block to create a hierarchical structure. Blocks
    can be repeated.
    """

    def add_operation(
        self, id: int, qubits: list[int], params: list[float] = []
    ) -> None:
        """
        Adds an operation to the block.

        Args:
            id (int): The operation ID.
            qubits (list[int]): The qubits involved in the operation.
            params (list[float]): The operation parameters.
        """
        ...

    def add_block(self, repetitions: int = 1) -> Block:
        """
        Adds a nested block to the block.

        Args:
            repetitions (int): The number of times the block is repeated.

        Returns:
            Block: The block.
        """
        ...

    def __str__(self) -> str:
        """
        Returns a string representation of the block.

        Returns:
            str: A string representation of the block.
        """
        ...

class PSSPC:
    def __new__(cls, num_ts_per_rotation: int, ccx_magic_states: bool) -> PSSPC: ...
    def transform(self, trace: Trace) -> Optional[Trace]: ...

class LatticeSurgery:
    def __new__(cls, slow_down_factor: float) -> LatticeSurgery: ...
    def transform(self, trace: Trace) -> Optional[Trace]: ...

class InstructionFrontier:
    """
    Represents a Pareto frontier of instructions with space, time, and error
    rates as objectives.
    """

    def __new__(cls) -> InstructionFrontier:
        """
        Creates a new instruction frontier.
        """
        ...

    def insert(self, point: _Instruction):
        """
        Inserts an instruction to the frontier.

        Args:
            point (_Instruction): The instruction to insert.
        """
        ...

    def extend(self, points: list[_Instruction]) -> None:
        """
        Extends the frontier with a list of instructions.

        Args:
            points (list[_Instruction]): The instructions to insert.
        """
        ...

    def __len__(self) -> int:
        """
        Returns the number of instructions in the frontier.

        Returns:
            int: The number of instructions.
        """
        ...

    def __iter__(self) -> Iterator[_Instruction]:
        """
        Returns an iterator over the instructions in the frontier.

        Returns:
            Iterator[_Instruction]: The iterator.
        """
        ...

    @staticmethod
    def load(filename: str) -> InstructionFrontier:
        """
        Loads an instruction frontier from a file.

        Args:
            filename (str): The file name.

        Returns:
            InstructionFrontier: The loaded instruction frontier.
        """
        ...

    def dump(self, filename: str) -> None:
        """
        Dumps the instruction frontier to a file.

        Args:
            filename (str): The file name.
        """
        ...

def _estimate_parallel(
    traces: list[Trace], isas: list[ISA], max_error: float = 1.0
) -> _EstimationCollection:
    """
    Estimates resources for multiple traces and ISAs in parallel.

    Args:
        traces (list[Trace]): The list of traces.
        isas (list[ISA]): The list of ISAs.
        max_error (float): The maximum allowed error. The default is 1.0.

    Returns:
        _EstimationCollection: The estimation collection.
    """
    ...

def _binom_ppf(q: float, n: int, p: float) -> int:
    """
    A replacement for SciPy's binom.ppf that is faster and does not require
    SciPy as a dependency.
    """
    ...

def instruction_name(id: int) -> Optional[str]:
    """
    Returns the name of an instruction given its ID, if known.

    Args:
        id (int): The instruction ID.

    Returns:
        Optional[str]: The name of the instruction, or None if the ID is not recognized.
    """
    ...
