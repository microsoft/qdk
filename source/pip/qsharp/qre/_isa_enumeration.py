# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations

import functools
import itertools
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from typing import Generator, Iterable

from ._architecture import Architecture
from ._qre import ISA


class Node(ABC):
    @abstractmethod
    def enumerate(self, ctx: Context) -> Generator[ISA, None, None]:
        """Yield all instances represented by this enumeration node."""
        pass

    def __add__(self, other: Node) -> SumNode:
        if isinstance(self, SumNode) and isinstance(other, SumNode):
            sources = self.sources + other.sources
            return SumNode(sources)
        elif isinstance(self, SumNode):
            sources = self.sources + [other]
            return SumNode(sources)
        elif isinstance(other, SumNode):
            sources = [self] + other.sources
            return SumNode(sources)
        else:
            return SumNode([self, other])

    def __mul__(self, other: Node) -> ProductNode:
        if isinstance(self, ProductNode) and isinstance(other, ProductNode):
            sources = self.sources + other.sources
            return ProductNode(sources)
        elif isinstance(self, ProductNode):
            sources = self.sources + [other]
            return ProductNode(sources)
        elif isinstance(other, ProductNode):
            sources = [self] + other.sources
            return ProductNode(sources)
        else:
            return ProductNode([self, other])

    def bind(self, name: str, node: Node) -> "BindingNode":
        """Create a BindingNode with this node as the component.

        Args:
            name: The name to bind the component to.
            node: The child enumeration node that may contain ISARefNodes.

        Returns:
            A BindingNode with self as the component.

        Example:
            ExampleErrorCorrection.q().bind("c", ISARefNode("c") * ISARefNode("c"))
        """
        return BindingNode(name=name, component=self, node=node)


@dataclass
class Context:
    """
    Context passed through enumeration, holding shared state.

    Attributes:
        architecture: The base architecture for enumeration.
    """

    architecture: Architecture
    _bindings: dict[str, ISA] = field(default_factory=dict, repr=False)

    @property
    def root_isa(self) -> ISA:
        """The architecture's provided ISA."""
        return self.architecture.provided_isa

    def _with_binding(self, name: str, isa: ISA) -> "Context":
        """Return a new context with an additional binding (internal use)."""
        new_bindings = {**self._bindings, name: isa}
        return Context(self.architecture, new_bindings)


@dataclass
class RootNode(Node):
    """
    Represents the architecture's base ISA.
    Reads from the context instead of holding a reference.
    """

    def enumerate(self, ctx: Context) -> Generator[ISA, None, None]:
        yield ctx.root_isa


# Singleton instance for convenience
ISA_ROOT = RootNode()


@dataclass
class ISAQuery(Node):
    component: type
    source: Node = field(default_factory=lambda: ISA_ROOT)
    kwargs: dict = field(default_factory=dict)

    def enumerate(self, ctx: Context) -> Generator[ISA, None, None]:
        for isa in self.source.enumerate(ctx):
            yield from self.component.enumerate_isas(isa, **self.kwargs)


@dataclass
class ProductNode(Node):
    sources: list[Node]

    def enumerate(self, ctx: Context) -> Generator[ISA, None, None]:
        source_generators = [source.enumerate(ctx) for source in self.sources]
        yield from product_isas(*source_generators)


@dataclass
class SumNode(Node):
    sources: list[Node]

    def enumerate(self, ctx: Context) -> Generator[ISA, None, None]:
        for source in self.sources:
            yield from source.enumerate(ctx)


@dataclass
class ISARefNode(Node):
    """
    A reference to a bound ISA in the enumeration context.

    This node looks up the binding from the context and yields the bound ISA.

    Args:
        name: The name of the bound ISA to reference.
    """

    name: str

    def enumerate(self, ctx: Context) -> Generator[ISA, None, None]:
        if self.name not in ctx._bindings:
            raise ValueError(f"Undefined component reference: '{self.name}'")
        yield ctx._bindings[self.name]


@dataclass
class BindingNode(Node):
    """
    Enumeration node that binds a component to a name.

    This node enables the as_/ref pattern where multiple positions in the
    enumeration tree share the same component instance. The bound component
    is enumerated once, and its value is shared across all ISARefNodes with
    the same name via the context.

    For multiple bindings, nest BindingNode instances.

    Args:
        name: The name to bind the component to.
        component: An EnumerationNode (e.g., ISAQuery) that produces the bound ISAs.
        node: The child enumeration node that may contain ISARefNodes.

    Example:
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
    component: Node
    node: Node

    def enumerate(self, ctx: Context) -> Generator[ISA, None, None]:
        # Enumerate all ISAs from the component node
        for isa in self.component.enumerate(ctx):
            # Add binding to context and enumerate child node
            new_ctx = ctx._with_binding(self.name, isa)
            yield from self.node.enumerate(new_ctx)


def product_isas(*isas: Iterable[ISA]) -> Iterable[ISA]:
    return (
        functools.reduce(lambda a, b: a + b, isa_tuple)
        for isa_tuple in itertools.product(*isas)
    )
