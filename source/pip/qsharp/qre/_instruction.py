# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations

from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from typing import Generator, Iterable, Optional
from enum import IntEnum

import pandas as pd

from ._architecture import ISAContext, Architecture
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
    Instruction,
    ISARequirements,
    instruction_name,
    property_name_to_key,
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
    **kwargs: bool,
) -> Constraint:
    """
    Create an instruction constraint.

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
            if (prop_key := property_name_to_key(key)) is None:
                raise ValueError(f"Unknown property '{key}'")

            c.add_property(prop_key)

    return c


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
        Return the requirements that an implementation ISA must satisfy.

        Returns:
            ISARequirements: The requirements for the underlying ISA.
        """
        ...

    @abstractmethod
    def provided_isa(
        self, impl_isa: ISA, ctx: ISAContext
    ) -> Generator[ISA, None, None]:
        """
        Yields ISAs provided by this transform given an implementation ISA.

        Args:
            impl_isa (ISA): The implementation ISA that satisfies requirements.
            ctx (ISAContext): The enumeration context whose provenance graph
                stores the instructions.

        Yields:
            ISA: A provided logical ISA.
        """
        ...

    @classmethod
    def enumerate_isas(
        cls,
        impl_isa: ISA | Iterable[ISA],
        ctx: ISAContext,
        **kwargs,
    ) -> Generator[ISA, None, None]:
        """
        Enumerate all valid ISAs for this transform given implementation ISAs.

        This method iterates over all instances of the transform class (enumerating
        hyperparameters) and filters implementation ISAs against requirements.

        Args:
            impl_isa (ISA | Iterable[ISA]): One or more implementation ISAs.
            ctx (ISAContext): The enumeration context.
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
        Create an ISAQuery node for this transform.

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
        Create a BindingNode for this transform.

        This is a convenience method equivalent to ``cls.q().bind(name, node)``.

        Args:
            name (str): The name to bind the transform's output to.
            node (Node): The child node that can reference this binding.

        Returns:
            BindingNode: A binding node enclosing this transform.
        """
        return cls.q().bind(name, node)


@dataclass(slots=True)
class InstructionSource:
    nodes: list[_InstructionSourceNode] = field(default_factory=list, init=False)
    roots: list[int] = field(default_factory=list, init=False)

    @classmethod
    def from_isa(cls, ctx: ISAContext, isa: ISA) -> InstructionSource:
        """
        Construct an InstructionSource graph from an ISA.

        The instruction source graph contains more information than the
        provenance graph in the context, as it connects the instructions to the
        transforms and architectures that generated them.

        Args:
            ctx (ISAContext): The enumeration context containing the provenance graph.
            isa (ISA): Instructions in the ISA will serve as root nodes in the source graph.

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
                ctx._provenance.instruction(source),
                ctx._transforms.get(ctx._provenance.transform_id(source)),
                children,
            )

            source_table[source] = node
            return node

        graph = cls()
        source_table: dict[int, int] = {}

        for inst in isa:
            node_idx = isa.node_index(inst.id)
            if node_idx is not None and node_idx != 0:
                node = _make_node(graph, source_table, node_idx)
                graph.add_root(node)

        return graph

    def add_root(self, node_id: int) -> None:
        """Add a root node to the instruction source graph.

        Args:
            node_id (int): The index of the node to add as a root.
        """
        self.roots.append(node_id)

    def add_node(
        self,
        instruction: Instruction,
        transform: Optional[ISATransform | Architecture],
        children: list[int],
    ) -> int:
        """Add a node to the instruction source graph.

        Args:
            instruction (Instruction): The instruction for this node.
            transform (Optional[ISATransform | Architecture]): The transform
                that produced the instruction.
            children (list[int]): Indices of child nodes.

        Returns:
            int: The index of the newly added node.
        """
        node_id = len(self.nodes)
        self.nodes.append(_InstructionSourceNode(instruction, transform, children))
        return node_id

    def __str__(self) -> str:
        """Return a formatted string representation of the instruction source graph."""

        def _format_node(node: _InstructionSourceNode, indent: int = 0) -> str:
            result = " " * indent + f"{instruction_name(node.instruction.id) or '??'}"
            if node.transform is not None:
                result += f" @ {node.transform}"
            for child_index in node.children:
                result += "\n" + _format_node(self.nodes[child_index], indent + 2)
            return result

        return "\n".join(
            _format_node(self.nodes[root_index]) for root_index in self.roots
        )

    def __getitem__(self, id: int) -> _InstructionSourceNodeReference:
        """
        Retrieve the first instruction source root node with the given
        instruction ID.  Raises KeyError if no such node exists.

        Args:
            id (int): The instruction ID to search for.

        Returns:
            _InstructionSourceNodeReference: The first instruction source node with the
                given instruction ID.
        """
        if (node := self.get(id)) is not None:
            return node

        raise KeyError(f"Instruction ID {id} not found in instruction source graph.")

    def __contains__(self, id: int) -> bool:
        """
        Check if there is an instruction source root node with the given
        instruction ID.

        Args:
            id (int): The instruction ID to search for.

        Returns:
            bool: True if a node with the given instruction ID exists, False otherwise.
        """
        for root in self.roots:
            if self.nodes[root].instruction.id == id:
                return True

        return False

    def get(
        self, id: int, default: Optional[_InstructionSourceNodeReference] = None
    ) -> Optional[_InstructionSourceNodeReference]:
        """
        Retrieve the first instruction source root node with the given
        instruction ID.  Returns default if no such node exists.

        Args:
            id (int): The instruction ID to search for.
            default (Optional[_InstructionSourceNodeReference]): The value to return if no
                node with the given ID is found. Default is None.

        Returns:
            Optional[_InstructionSourceNodeReference]: The first instruction source node with the
                given instruction ID, or default if no such node exists.
        """
        for root in self.roots:
            if self.nodes[root].instruction.id == id:
                return _InstructionSourceNodeReference(self, root)

        return default


@dataclass(frozen=True, slots=True)
class _InstructionSourceNode:
    """A node in the instruction source graph."""

    instruction: Instruction
    transform: Optional[ISATransform | Architecture]
    children: list[int]


class _InstructionSourceNodeReference:
    """Reference to a node in an InstructionSource graph."""

    def __init__(self, graph: InstructionSource, node_id: int):
        """Initialize a reference to a node in the instruction source graph.

        Args:
            graph (InstructionSource): The owning instruction source graph.
            node_id (int): The index of the referenced node.
        """
        self.graph = graph
        self.node_id = node_id

    @property
    def instruction(self) -> Instruction:
        """The instruction at this node."""
        return self.graph.nodes[self.node_id].instruction

    @property
    def transform(self) -> Optional[ISATransform | Architecture]:
        """The transform that produced this node's instruction, if any."""
        return self.graph.nodes[self.node_id].transform

    def __str__(self) -> str:
        """Return a string representation of the referenced node."""
        return str(self.graph.nodes[self.node_id])

    def __getitem__(self, id: int) -> _InstructionSourceNodeReference:
        """
        Retrieve the first child instruction source node with the given
        instruction ID.  Raises KeyError if no such node exists.

        Args:
            id (int): The instruction ID to search for.

        Returns:
            _InstructionSourceNodeReference: The first child instruction source node with the
                given instruction ID.
        """
        if (node := self.get(id)) is not None:
            return node

        raise KeyError(
            f"Instruction ID {id} not found in children of instruction {instruction_name(self.instruction.id) or '??'}."
        )

    def get(
        self, id: int, default: Optional[_InstructionSourceNodeReference] = None
    ) -> Optional[_InstructionSourceNodeReference]:
        """
        Retrieve the first child instruction source node with the given
        instruction ID.  Returns default if no such node exists.

        Args:
            id (int): The instruction ID to search for.
            default (Optional[_InstructionSourceNodeReference]): The value to return if no
                node with the given ID is found. Default is None.

        Returns:
            Optional[_InstructionSourceNodeReference]: The first child instruction source
                node with the given instruction ID, or default if no such node
                exists.
        """

        for child_id in self.graph.nodes[self.node_id].children:
            if self.graph.nodes[child_id].instruction.id == id:
                return _InstructionSourceNodeReference(self.graph, child_id)

        return default


def _isa_as_frame(self: ISA) -> pd.DataFrame:
    """Convert an ISA to a pandas DataFrame.

    Args:
        self (ISA): The ISA to convert.

    Returns:
        pd.DataFrame: A DataFrame with columns for id, encoding, arity,
            space, time, and error.
    """
    data = {
        "id": [instruction_name(inst.id) for inst in self],
        "encoding": [Encoding(inst.encoding).name for inst in self],
        "arity": [inst.arity for inst in self],
        "space": [
            inst.expect_space() if inst.arity is not None else None for inst in self
        ],
        "time": [
            inst.expect_time() if inst.arity is not None else None for inst in self
        ],
        "error": [
            inst.expect_error_rate() if inst.arity is not None else None
            for inst in self
        ],
    }

    df = pd.DataFrame(data)
    df.set_index("id", inplace=True)
    return df


def _requirements_as_frame(self: ISARequirements) -> pd.DataFrame:
    """Convert ISA requirements to a pandas DataFrame.

    Args:
        self (ISARequirements): The requirements to convert.

    Returns:
        pd.DataFrame: A DataFrame with columns for id, encoding, and arity.
    """
    data = {
        "id": [instruction_name(inst.id) for inst in self],
        "encoding": [Encoding(inst.encoding).name for inst in self],
        "arity": [inst.arity for inst in self],
    }

    df = pd.DataFrame(data)
    df.set_index("id", inplace=True)
    return df
