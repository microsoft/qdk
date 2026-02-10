# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations

from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from typing import Generator, Iterable, Optional, overload, cast
from enum import IntEnum

from ._architecture import _Context, Architecture
from ._enumeration import _enumerate_instances
from ._isa_enumeration import (
    ISA_ROOT,
    _BindingNode,
    _ComponentQuery,
    ISAQuery,
)
from ._qre import (
    ISA,
    Constraint,
    ConstraintBound,
    EstimationResult,
    _FloatFunction,
    _Instruction,
    _IntFunction,
    ISARequirements,
    constant_function,
    instruction_name,
)


class Encoding(IntEnum):
    PHYSICAL = 0
    LOGICAL = 1


class PropertyKey(IntEnum):
    DISTANCE = 0


PHYSICAL = Encoding.PHYSICAL
LOGICAL = Encoding.LOGICAL


def constraint(
    id: int,
    encoding: Encoding = PHYSICAL,
    *,
    arity: Optional[int] = 1,
    error_rate: Optional[ConstraintBound] = None,
    **kwargs: bool,
) -> Constraint:
    """
    Creates an instruction constraint.

    Args:
        id (int): The instruction ID.
        encoding (Encoding): The instruction encoding. PHYSICAL (0) or LOGICAL (1).
        arity (Optional[int]): The instruction arity. If None, instruction is
            assumed to have variable arity.  Default is 1.
        error_rate (Optional[ConstraintBound]): The constraint on the error rate.
        **kwargs (bool): Required properties that matching instructions must have.
            Valid property names: distance. Set to True to require the property.

    Returns:
        Constraint: The instruction constraint.

    Raises:
        ValueError: If an unknown property name is provided in kwargs.
    """
    c = Constraint(id, encoding, arity, error_rate)

    for key, value in kwargs.items():
        if value:
            try:
                prop_key = PropertyKey[key.upper()]
            except KeyError:
                raise ValueError(
                    f"Unknown property '{key}'. Valid properties: {[k.name.lower() for k in PropertyKey]}"
                )
            c.add_property(prop_key)

    return c


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
    **kwargs: int,
) -> _Instruction: ...
@overload
def instruction(
    id: int,
    encoding: Encoding = PHYSICAL,
    *,
    time: int | _IntFunction,
    arity: None = ...,
    space: int | _IntFunction,
    length: Optional[int | _IntFunction] = None,
    error_rate: float | _FloatFunction,
    **kwargs: int,
) -> _Instruction: ...
def instruction(
    id: int,
    encoding: Encoding = PHYSICAL,
    *,
    time: int | _IntFunction,
    arity: Optional[int] = 1,
    space: Optional[int] | _IntFunction = None,
    length: Optional[int | _IntFunction] = None,
    error_rate: float | _FloatFunction,
    **kwargs: int,
) -> _Instruction:
    """
    Creates an instruction.

    Args:
        id (int): The instruction ID.
        encoding (Encoding): The instruction encoding. PHYSICAL (0) or LOGICAL (1).
        time (int | _IntFunction): The instruction time in ns.
        arity (Optional[int]): The instruction arity.  If None, instruction is
            assumed to have variable arity.  Default is 1.  One can use variable arity
            functions for time, space, length, and error_rate in this case.
        space (Optional[int] | _IntFunction): The instruction space in number of
            physical qubits. If None, length is used.
        length (Optional[int | _IntFunction]): The arity including ancilla
            qubits. If None, arity is used.
        error_rate (float | _FloatFunction): The instruction error rate.
        **kwargs (int): Additional properties to set on the instruction.
            Valid property names: distance.

    Returns:
        _Instruction: The instruction.

    Raises:
        ValueError: If an unknown property name is provided in kwargs.
    """
    if arity is not None:
        instr = _Instruction.fixed_arity(
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

        instr = _Instruction.variable_arity(
            id,
            encoding,
            time,
            cast(_IntFunction, space),
            cast(_FloatFunction, error_rate),
            length,
        )

    for key, value in kwargs.items():
        try:
            prop_key = PropertyKey[key.upper()]
        except KeyError:
            raise ValueError(
                f"Unknown property '{key}'. Valid properties: {[k.name.lower() for k in PropertyKey]}"
            )
        instr.set_property(prop_key, value)

    return instr


class ISATransform(ABC):
    """
    Abstract base class for transformations between ISAs (e.g., QEC schemes).

    An ISA transform defines a mapping from a required input ISA (e.g.,
    architecture constraints) to a provided output ISA (logical instructions).
    It supports enumeration of configuration parameters.
    """

    @staticmethod
    @abstractmethod
    def required_isa() -> ISARequirements:
        """
        Returns the requirements that an implementation ISA must satisfy.

        Returns:
            ISARequirements: The requirements for the underlying ISA.
        """
        ...

    @abstractmethod
    def provided_isa(self, impl_isa: ISA, ctx: _Context) -> Generator[ISA, None, None]:
        """
        Yields ISAs provided by this transform given an implementation ISA.

        Args:
            impl_isa (ISA): The implementation ISA that satisfies requirements.

        Yields:
            ISA: A provided logical ISA.
        """
        ...

    @classmethod
    def enumerate_isas(
        cls,
        impl_isa: ISA | Iterable[ISA],
        ctx: _Context,
        **kwargs,
    ) -> Generator[ISA, None, None]:
        """
        Enumerates all valid ISAs for this transform given implementation ISAs.

        This method iterates over all instances of the transform class (enumerating
        hypterparameters) and filters implementation ISAs against requirements.

        Args:
            impl_isa (ISA | Iterable[ISA]): One or more implementation ISAs.
            **kwargs: Arguments passed to parameter enumeration.

        Yields:
            ISA: Valid provided ISAs.
        """
        isas = [impl_isa] if isinstance(impl_isa, ISA) else impl_isa
        for isa in isas:
            if not isa.satisfies(cls.required_isa()):
                continue

            for component in _enumerate_instances(cls, **kwargs):
                ctx._transforms[id(component)] = component
                yield from component.provided_isa(isa, ctx)

    @classmethod
    def q(cls, *, source: ISAQuery | None = None, **kwargs) -> ISAQuery:
        """
        Creates an ISAQuery node for this transform.

        Args:
            source (Node | None): The source node providing implementation ISAs.
                Defaults to ISA_ROOT.
            **kwargs: Additional arguments for parameter enumeration.

        Returns:
            ISAQuery: An enumeration node representing this transform.
        """
        return _ComponentQuery(
            cls, source=source if source is not None else ISA_ROOT, kwargs=kwargs
        )

    @classmethod
    def bind(cls, name: str, node: ISAQuery) -> _BindingNode:
        """
        Creates a BindingNode for this transform.

        This is a convenience method equivalent to `cls.q().bind(name, node)`.

        Args:
            name (str): The name to bind the transform's output to.
            node (Node): The child node that can reference this binding.

        Returns:
            BindingNode: A binding node enclosing this transform.
        """
        return cls.q().bind(name, node)


@dataclass(frozen=True, slots=True)
class InstructionSource:
    nodes: list[_InstructionSourceNode] = field(default_factory=list, init=False)
    roots: list[int] = field(default_factory=list, init=False)

    @classmethod
    def from_estimation_result(
        cls, ctx: _Context, result: EstimationResult
    ) -> InstructionSource:
        """
        Constructs an InstructionSource graph from an EstimationResult.

        The instruction source graph contains more information than the
        provenance graph in the context, as it connects the instructions to the
        transforms and architectures that generated them.

        Args:
            ctx (_Context): The enumeration context containing the provenance graph.
            result (EstimationResult): The estimation result containing the ISA and instruction sources.

        Returns:
            InstructionSource: The instruction source graph for the estimation result.
        """

        def _make_node(
            graph: InstructionSource, source_table: dict[int, int], source: int
        ) -> int:
            if source in source_table:
                return source_table[source]

            children = [
                _make_node(graph, source_table, child)
                for child in ctx._provenance.children(source)
                if child != 0
            ]

            node = graph.add_node(
                ctx._provenance.instruction_id(source),
                ctx._transforms.get(ctx._provenance.transform_id(source)),
                children,
            )

            source_table[source] = node
            return node

        graph = cls()
        source_table: dict[int, int] = {}

        for inst in result.isa:
            if inst.source != 0:
                node = _make_node(graph, source_table, inst.source)
                graph.add_root(node)

        return graph

    def add_root(self, node_id: int) -> None:
        self.roots.append(node_id)

    def add_node(
        self,
        id: int,
        transform: Optional[ISATransform | Architecture],
        children: list[int],
    ) -> int:
        node_id = self.nodes.__len__()
        self.nodes.append(_InstructionSourceNode(id, transform, children))
        return node_id

    def __str__(self) -> str:
        def _format_node(node: _InstructionSourceNode, indent: int = 0) -> str:
            result = " " * indent + f"{instruction_name(node.id) or '??'}"
            if node.transform is not None:
                result += f" @ {node.transform}"
            for child_index in node.children:
                result += "\n" + _format_node(self.nodes[child_index], indent + 2)
            return result

        return "\n".join(
            _format_node(self.nodes[root_index]) for root_index in self.roots
        )


@dataclass(frozen=True, slots=True)
class _InstructionSourceNode:
    id: int
    transform: Optional[ISATransform | Architecture]
    children: list[int]
