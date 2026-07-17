# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations

import copy
import dataclasses
from abc import ABC, abstractmethod
from enum import IntEnum
from typing import TYPE_CHECKING, cast

from ._qre import (
    ISA,
    Instruction,
    _FloatFunction,
    _IntFunction,
    _ProvenanceGraph,
    constant_function,
    property_name_to_key,
)
from .instruction_ids import INSTRUCTION_ID_MAP

if TYPE_CHECKING:
    from typing import Any, Optional, Type, Union

    from ._instruction import Encoding, ISATransform


class TechnologyFamily(IntEnum):
    UNKNOWN = 0
    TOPOLOGICAL = 1
    SUPERCONDUCTING = 2
    ION_TRAP = 3
    NEUTRAL_ATOM = 4
    SOLID_STATE = 5


class Architecture(ABC):
    """Abstract base class for quantum hardware architectures."""

    family: TechnologyFamily = TechnologyFamily.UNKNOWN

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

    @property
    def assumptions(self) -> list[str]:
        """
        A list of assumptions for the architecture.

        Returns:
            list[str]: A list of assumptions.
        """
        return []

    @property
    def references(self) -> list[str]:
        """
        A list of technical references for the architecture.

        Returns:
            list[str]: A list of technical references.
        """
        return []


def qubit(
    base_class: Optional[Union[Type[Architecture], type]] = None,
    name: Optional[str] = None,
    *,
    family: TechnologyFamily = TechnologyFamily.UNKNOWN,
) -> Any:
    """Decorator that turns a plain class into an Architecture subclass.

    Args:
        base_class: An Architecture subclass to inherit from.  If ``None``,
            ``provided_isa`` is auto-generated from instruction ID attributes.
        name: Human-readable name.  Defaults to the class name.
        family: The technology family this qubit belongs to.

    Returns:
        The returned type is intentionally ``Any`` because the generated class
        combines the decorated class with a dynamically chosen ``Architecture``
        base, which cannot be expressed statically.

    Examples::

        @qubit(Majorana, family=TechnologyFamily.TOPOLOGICAL)
        class MajoranaQubit:
            error_rate: float = 1e-6

        @qubit(family=TechnologyFamily.SUPERCONDUCTING)
        class FastGoodQubit:
            CNOT = {"arity": 2, "time": 100, "error_rate": 1e-6}
    """

    def decorator(cls: type) -> Any:
        nonlocal base_class, name

        actual_base = base_class if base_class is not None else Architecture
        actual_name = name if name is not None else cls.__name__

        attrs = {
            key: value for key, value in cls.__dict__.items() if not key.startswith("_")
        }
        annotations = dict(getattr(cls, "__annotations__", {}))

        # When the base is a dataclass, promote bare attributes that match a
        # parent dataclass field into annotations so they become proper field
        # overrides rather than plain class attributes shadowed by __init__.
        if dataclasses.is_dataclass(actual_base):
            parent_fields = {f.name: f for f in dataclasses.fields(actual_base)}
            for attr_name, value in list(attrs.items()):
                if attr_name in parent_fields and attr_name not in annotations:
                    annotations[attr_name] = parent_fields[attr_name].type

        # When no base class provides provided_isa, generate one from
        # attributes that match instruction ID names.
        if base_class is None:
            instruction_attrs: dict[str, dict] = {}
            for attr_name in list(attrs):
                if attr_name in INSTRUCTION_ID_MAP and isinstance(
                    attrs[attr_name], dict
                ):
                    instruction_attrs[attr_name] = attrs.pop(attr_name)
                    annotations.pop(attr_name, None)

            def _provided_isa(
                self: Architecture,
                ctx: ISAContext,
                _instr: dict[str, dict] = instruction_attrs,
            ) -> ISA:
                sources = []
                for instr_name, kwargs in _instr.items():
                    instr_id = INSTRUCTION_ID_MAP[instr_name]
                    sources.append(ctx.add_instruction(instr_id, **kwargs))
                return ctx.make_isa(*sources)

            attrs["provided_isa"] = _provided_isa

        attrs["__annotations__"] = annotations
        attrs["__str__"] = lambda self, n=actual_name: n
        attrs["__qualname__"] = cls.__qualname__
        attrs["__module__"] = cls.__module__

        attrs["family"] = family

        new_cls = type(cls.__name__, (actual_base,), attrs)

        if dataclasses.is_dataclass(actual_base):
            new_cls = dataclasses.dataclass(new_cls)

        # Register the model for auto-derived target lookup.
        _register_qubit_model(new_cls, actual_name)

        return new_cls

    # Support @qubit without parentheses (bare decorator on a class).
    if (
        base_class is not None
        and isinstance(base_class, type)
        and not issubclass(base_class, Architecture)
    ):
        # Called as @qubit with no args — base_class is actually the class.
        cls = base_class
        base_class = None
        name = None
        return decorator(cls)

    return decorator


# ---------------------------------------------------------------------------
# Global qubit model registry — populated by the @qubit decorator.
# Maps a model's display name to its qubit model class.
# ---------------------------------------------------------------------------

QUBIT_MODELS: dict[str, type] = {}


def _register_qubit_model(cls: type, display_name: str) -> None:
    """Register a qubit model class under its display name.

    Args:
        cls (type): The qubit model class to register.
        display_name (str): The name to register the model under.

    Raises:
        ValueError: If a different model is already registered under
            ``display_name``.
    """
    existing = QUBIT_MODELS.get(display_name)
    if existing is not None and existing is not cls:
        raise ValueError(f"A qubit model named {display_name!r} is already registered.")
    QUBIT_MODELS[display_name] = cls


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
        **kwargs: Optional[int],
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
                when passing an existing ``Instruction``, or when the value is
                ``None``.

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
    properties: dict[str, Optional[int]],
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
        if value is None:
            continue
        prop_key = property_name_to_key(key)
        if prop_key is None:
            raise ValueError(f"Unknown property '{key}'.")
        instr.set_property(prop_key, value)

    return instr
