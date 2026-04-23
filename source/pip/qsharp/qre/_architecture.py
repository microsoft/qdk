# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations
import copy
from typing import cast, TYPE_CHECKING

from abc import ABC, abstractmethod

from ._qre import (
    ISA,
    _ProvenanceGraph,
    Instruction,
    _IntFunction,
    _FloatFunction,
    constant_function,
    property_name_to_key,
)

if TYPE_CHECKING:
    from typing import Optional

    from ._instruction import ISATransform, Encoding


class Architecture(ABC):
    """Abstract base class for quantum hardware architectures."""

    @abstractmethod
    def provided_isa(self, ctx: ISAContext) -> ISA:
        """
        Create the ISA provided by this architecture, adding instructions
        directly to the context's provenance graph.

        Args:
            ctx (ISAContext): The enumeration context whose provenance graph stores
                the instructions.

        Returns:
            ISA: The ISA backed by the context's provenance graph.
        """
        ...

    def context(self) -> ISAContext:
        """Create a new enumeration context for this architecture.

        Returns:
            ISAContext: A new enumeration context.
        """
        return ISAContext(self)


class ISAContext:
    """
    Context passed through enumeration, holding shared state.
    """

    def __init__(self, arch: Architecture):
        """Initialize the ISA context for the given architecture.

        Args:
            arch (Architecture): The architecture providing the base ISA.
        """
        self._provenance: _ProvenanceGraph = _ProvenanceGraph()

        # Let the architecture create instructions directly in the graph.
        self._isa = arch.provided_isa(self)

        self._bindings: dict[str, ISA] = {}
        self._transforms: dict[int, Architecture | ISATransform] = {0: arch}

    def _with_binding(self, name: str, isa: ISA) -> ISAContext:
        """Return a new context with an additional binding (internal use)."""
        ctx = copy.copy(self)
        ctx._bindings = {**self._bindings, name: isa}
        return ctx

    @property
    def isa(self) -> ISA:
        """The ISA provided by the architecture for this context."""
        return self._isa

    def add_instruction(
        self,
        id_or_instruction: int | Instruction,
        encoding: Encoding = 0,  # type: ignore
        *,
        arity: Optional[int] = 1,
        time: int | _IntFunction = 0,
        space: Optional[int] | _IntFunction = None,
        length: Optional[int | _IntFunction] = None,
        error_rate: float | _FloatFunction = 0.0,
        transform: ISATransform | None = None,
        source: list[Instruction] | None = None,
        **kwargs: int,
    ) -> int:
        """
        Create an instruction and add it to the provenance graph.

        Can be called in two ways:

        1. With keyword args to create a new instruction::

              ctx.add_instruction(T, encoding=LOGICAL, time=1000,
                                  error_rate=1e-8)

        2. With a pre-existing ``Instruction`` object (e.g. from
           ``with_id()``)::

              ctx.add_instruction(existing_instruction)

        Provenance is recorded when *transform* and/or *source* are
        supplied:

        - **transform** — the ``ISATransform`` that produced the
          instruction.
        - **source** — input instructions consumed by the transform.

        Args:
            id_or_instruction: Either an instruction ID (int) for creating
                a new instruction, or an existing ``Instruction`` object.
            encoding: The instruction encoding (0 = Physical, 1 = Logical).
                Ignored when passing an existing ``Instruction``.
            arity: The instruction arity. ``None`` for variable arity.
                Ignored when passing an existing ``Instruction``.
            time: Instruction time in ns (or ``_IntFunction`` for variable
                arity). Ignored when passing an existing ``Instruction``.
            space: Instruction space in physical qubits (or ``_IntFunction``
                for variable arity). Ignored when passing an existing
                ``Instruction``.
            length: Arity including ancilla qubits. Ignored when passing an
                existing ``Instruction``.
            error_rate: Instruction error rate (or ``_FloatFunction`` for
                variable arity). Ignored when passing an existing
                ``Instruction``.
            transform: The ``ISATransform`` that produced the instruction.
            source: List of source ``Instruction`` objects consumed by the
                transform.
            **kwargs: Additional properties (e.g. ``distance=9``). Ignored
                when passing an existing ``Instruction``.

        Returns:
            The node index in the provenance graph.

        Raises:
            ValueError: If an unknown property name is provided in kwargs.
        """
        if transform is None and source is None:
            return self._provenance.add_instruction(
                cast(int, id_or_instruction),
                encoding,
                arity=arity,
                time=time,
                space=space,
                length=length,
                error_rate=error_rate,
                **kwargs,
            )

        if isinstance(id_or_instruction, Instruction):
            instr = id_or_instruction
        else:
            instr = _make_instruction(
                id_or_instruction,
                int(encoding),
                arity,
                time,
                space,
                length,
                error_rate,
                kwargs,
            )

        transform_id = id(transform) if transform is not None else 0
        children = [inst.source for inst in source] if source else []

        node_index = self._provenance.add_node(instr, transform_id, children)

        if transform is not None:
            self._transforms[transform_id] = transform

        return node_index

    def make_isa(self, *node_indices: int) -> ISA:
        """
        Create an ISA backed by this context's provenance graph from the
        given node indices.

        Args:
            *node_indices (int): Node indices in the provenance graph.

        Returns:
            ISA: An ISA referencing the provenance graph.
        """
        return self._provenance.make_isa(list(node_indices))


def _make_instruction(
    id: int,
    encoding: int,
    arity: int | None,
    time: int | _IntFunction,
    space: int | _IntFunction | None,
    length: int | _IntFunction | None,
    error_rate: float | _FloatFunction,
    properties: dict[str, int],
) -> Instruction:
    """Build an ``Instruction`` from keyword arguments."""
    if arity is not None:
        instr = Instruction.fixed_arity(
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
        if isinstance(error_rate, (int, float)):
            error_rate = constant_function(float(error_rate))

        instr = Instruction.variable_arity(
            id,
            encoding,
            time,
            cast(_IntFunction, space),
            error_rate,
            length,
        )

    for key, value in properties.items():
        prop_key = property_name_to_key(key)
        if prop_key is None:
            raise ValueError(f"Unknown property '{key}'.")
        instr.set_property(prop_key, value)

    return instr
