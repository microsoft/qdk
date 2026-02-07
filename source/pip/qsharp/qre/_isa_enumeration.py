# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations

import functools
import itertools
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from typing import Generator

from ._architecture import _Context
from ._qre import ISA


class ISAQuery(ABC):
    """
    Abstract base class for all nodes in the ISA enumeration tree.

    Enumeration nodes define the structure of the search space for ISAs starting
    from architectures and mofied by ISA transforms such as error correction
    schemes. They can be composed using operators like `+` (sum) and `*`
    (product) to build complex enumeration strategies.
    """

    @abstractmethod
    def enumerate(self, ctx: _Context) -> Generator[ISA, None, None]:
        """
        Yields all ISA instances represented by this enumeration node.

        Args:
            ctx (Context): The enumeration context containing shared state,
            e.g., access to the underlying architecture.

        Yields:
            ISA: A possible ISA that can be generated from this node.
        """
        pass

    def __add__(self, other: ISAQuery) -> _SumNode:
        """
        Performs a union of two enumeration nodes.

        Enumerating the sum node yields all ISAs from this node, followed by all
        ISAs from the other node.  Duplicate ISAs may be produced if both nodes
        yield the same ISA.

        Args:
            other (Node): The other enumeration node.

        Returns:
            SumNode: A node representing the union of both enumerations.

        Example:

            The following enumerates ISAs from both SurfaceCode and ColorCode:

        .. code-block:: python
            for isa in SurfaceCode.q() + ColorCode.q():
                ...
        """
        if isinstance(self, _SumNode) and isinstance(other, _SumNode):
            sources = self.sources + other.sources
            return _SumNode(sources)
        elif isinstance(self, _SumNode):
            sources = self.sources + [other]
            return _SumNode(sources)
        elif isinstance(other, _SumNode):
            sources = [self] + other.sources
            return _SumNode(sources)
        else:
            return _SumNode([self, other])

    def __mul__(self, other: ISAQuery) -> _ProductNode:
        """
        Performs the cross product of two enumeration nodes.

        Enumerating the product node yields ISAs resulting from the Cartesian
        product of ISAs from both nodes. The ISAs are combined using
        concatenation (logical union).  This means that instructions in the
        other enumeration node with the same ID as an instruction in this
        enumeration node will overwrite the instruction from this node.

        Args:
            other (Node): The other enumeration node.

        Returns:
            ProductNode: A node representing the product of both enumerations.

        Example:

            The following enumerates ISAs formed by combining ISAs from a
            surface code and a factory:

        .. code-block:: python

            for isa in SurfaceCode.q() * Factory.q():
                ...
        """
        if isinstance(self, _ProductNode) and isinstance(other, _ProductNode):
            sources = self.sources + other.sources
            return _ProductNode(sources)
        elif isinstance(self, _ProductNode):
            sources = self.sources + [other]
            return _ProductNode(sources)
        elif isinstance(other, _ProductNode):
            sources = [self] + other.sources
            return _ProductNode(sources)
        else:
            return _ProductNode([self, other])

    def bind(self, name: str, node: ISAQuery) -> "_BindingNode":
        """Create a BindingNode with this node as the component.

        Args:
            name: The name to bind the component to.
            node: The child enumeration node that may contain ISARefNodes.

        Returns:
            A BindingNode with self as the component.

        Example:

        .. code-block:: python
            ExampleErrorCorrection.q().bind("c", ISARefNode("c") * ISARefNode("c"))
        """
        return _BindingNode(name=name, component=self, node=node)


@dataclass
class RootNode(ISAQuery):
    """
    Represents the architecture's base ISA.
    Reads from the context instead of holding a reference.
    """

    def enumerate(self, ctx: _Context) -> Generator[ISA, None, None]:
        """
        Yields the architecture ISA from the context.

        Args:
            ctx (Context): The enumeration context.

        Yields:
            ISA: The architecture's provided ISA, called root.
        """
        yield ctx.root_isa


# Singleton instance for convenience
ISA_ROOT = RootNode()


@dataclass
class _ComponentQuery(ISAQuery):
    """
    Query node that enumerates ISAs based on a component type and source.

    This node takes a component type (which must have an `enumerate_isas` class
    method) and a source node. It enumerates the source node to get base ISAs,
    and then calls `enumerate_isas` on the component type for each base ISA
    to generate derived ISAs.

    Attributes:
        component: The component type to query (e.g., a QEC code class).
        source: The source node providing input ISAs (default: ISA_ROOT).
        kwargs: Additional keyword arguments passed to `enumerate_isas`.
    """

    component: type
    source: ISAQuery = field(default_factory=lambda: ISA_ROOT)
    kwargs: dict = field(default_factory=dict)

    def enumerate(self, ctx: _Context) -> Generator[ISA, None, None]:
        """
        Yields ISAs generated by the component from source ISAs.

        Args:
            ctx (Context): The enumeration context.

        Yields:
            ISA: A generated ISA instance.
        """
        for isa in self.source.enumerate(ctx):
            yield from self.component.enumerate_isas(isa, **self.kwargs)


@dataclass
class _ProductNode(ISAQuery):
    """
    Node representing the Cartesian product of multiple source nodes.

    Attributes:
        sources: A list of source nodes to combine.
    """

    sources: list[ISAQuery]

    def enumerate(self, ctx: _Context) -> Generator[ISA, None, None]:
        """
        Yields ISAs formed by combining ISAs from all source nodes.

        Args:
            ctx (Context): The enumeration context.

        Yields:
            ISA: A combined ISA instance.
        """
        source_generators = [source.enumerate(ctx) for source in self.sources]
        yield from (
            functools.reduce(lambda a, b: a + b, isa_tuple)
            for isa_tuple in itertools.product(*source_generators)
        )


@dataclass
class _SumNode(ISAQuery):
    """
    Node representing the union of multiple source nodes.

    Attributes:
        sources: A list of source nodes to enumerate sequentially.
    """

    sources: list[ISAQuery]

    def enumerate(self, ctx: _Context) -> Generator[ISA, None, None]:
        """
        Yields ISAs from each source node in sequence.

        Args:
            ctx (Context): The enumeration context.

        Yields:
            ISA: An ISA instance from one of the sources.
        """
        for source in self.sources:
            yield from source.enumerate(ctx)


@dataclass
class ISARefNode(ISAQuery):
    """
    A reference to a bound ISA in the enumeration context.

    This node looks up the binding from the context and yields the bound ISA.

    Args:
        name: The name of the bound ISA to reference.
    """

    name: str

    def enumerate(self, ctx: _Context) -> Generator[ISA, None, None]:
        """
        Yields the bound ISA from the context.

        Args:
            ctx (Context): The enumeration context containing bindings.

        Yields:
            ISA: The bound ISA.

        Raises:
            ValueError: If the name is not bound in the context.
        """
        if self.name not in ctx._bindings:
            raise ValueError(f"Undefined component reference: '{self.name}'")
        yield ctx._bindings[self.name]


@dataclass
class _BindingNode(ISAQuery):
    """
    Enumeration node that binds a component to a name.

    This node enables the as_/ref pattern where multiple positions in the
    enumeration tree share the same component instance. The bound component
    is enumerated once, and its value is shared across all ISARefNodes with
    the same name via the context.

    For multiple bindings, nest BindingNode instances.

    Args:
        name: The name to bind the component to.
        component: An EnumerationNode (e.g., _ComponentQuery) that produces the bound ISAs.
        node: The child enumeration node that may contain ISARefNodes.

    Example:

    .. code-block:: python
        ctx = EnumerationContext(architecture=arch)

        # Bind a code and reference it multiple times
        BindingNode(
            name="c",
            component=ExampleErrorCorrection.q(),
            node=ISARefNode("c") * ISARefNode("c"),
        ).enumerate(ctx)

        # Multiple bindings via nesting
        BindingNode(
            name="c",
            component=ExampleErrorCorrection.q(),
            node=BindingNode(
                name="f",
                component=ExampleFactory.q(source=ISARefNode("c")),
                node=ISARefNode("c") * ISARefNode("f"),
            ),
        ).enumerate(ctx)
    """

    name: str
    component: ISAQuery
    node: ISAQuery

    def enumerate(self, ctx: _Context) -> Generator[ISA, None, None]:
        """
        Enumerates child nodes with the bound component in context.

        Args:
            ctx (Context): The enumeration context.

        Yields:
            ISA: An ISA instance from the child node.
        """
        # Enumerate all ISAs from the component node
        for isa in self.component.enumerate(ctx):
            # Add binding to context and enumerate child node
            new_ctx = ctx._with_binding(self.name, isa)
            yield from self.node.enumerate(new_ctx)
