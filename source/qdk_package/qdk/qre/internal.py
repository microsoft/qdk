# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Internal types that appear in the ``qdk.qre`` public API surface.

Warning:
    The types exposed here are **not** part of the supported public API
    and may change in any release without notice.  They are made reachable
    from this module solely so that:

    1. Documentation generators (py2docfx, Sphinx) can emit working
       cross-reference links for return types and parameter types.
    2. Type checkers (pyright, mypy) do not flag references as
       private-module accesses when users annotate variables that hold
       values returned by public functions.
    3. Users who follow a type annotation can land on a clearly-labeled
       page rather than a ``ModuleNotFoundError``.

    Do **not** depend on the presence or shape of any symbol in this
    module.  If you need to construct or configure one of these types
    directly, use the corresponding public API instead.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from typing import ClassVar, Optional, Protocol, Union

    from ._application import Application
    from ._architecture import Architecture
    from ._instruction import ISATransform

    # ------------------------------------------------------------------
    # ApplicationContext
    #   (runtime: _application.ApplicationContext)
    # ------------------------------------------------------------------
    class ApplicationContext(Protocol):
        """Enumeration context wrapping an application instance.

        Obtained via :meth:`~qdk.qre.Application.context` and passed to
        :meth:`~qdk.qre.TraceQuery.enumerate`.
        """

        @property
        def application(self) -> "Application": ...

    # ------------------------------------------------------------------
    # DataclassProtocol
    #   (runtime: _application.DataclassProtocol)
    # ------------------------------------------------------------------
    class DataclassProtocol(Protocol):
        """Structural type satisfied by any ``@dataclass`` class.

        Used as a constraint on :data:`~qdk.qre.TraceParameters`.
        """

        __dataclass_fields__: ClassVar[dict]

    # ------------------------------------------------------------------
    # InstructionSourceNodeReference
    #   (runtime: _instruction._InstructionSourceNodeReference)
    # ------------------------------------------------------------------
    class InstructionSourceNodeReference(Protocol):
        """Reference to a node in an :class:`~qdk.qre.InstructionSource` graph."""

        @property
        def instruction(self) -> Instruction: ...
        @property
        def transform(self) -> Union[ISATransform, Architecture, None]: ...

    # ------------------------------------------------------------------
    # Instruction  (runtime: _qre.Instruction — Rust native)
    # ------------------------------------------------------------------
    class Instruction(Protocol):
        """A quantum instruction with resource properties."""

        @property
        def id(self) -> int: ...
        @property
        def encoding(self) -> int: ...
        @property
        def arity(self) -> int | None: ...
        def space(self, arity: int | None = None) -> int | None: ...
        def time(self, arity: int | None = None) -> int | None: ...
        def error_rate(self, arity: int | None = None) -> float | None: ...
        def expect_time(self, arity: int | None = None) -> int: ...
        def expect_error_rate(self, arity: int | None = None) -> float: ...

else:
    from ._application import ApplicationContext, DataclassProtocol
    from ._instruction import (
        _InstructionSourceNodeReference as InstructionSourceNodeReference,
    )
    from ._qre import Instruction

__all__ = [
    "ApplicationContext",
    "DataclassProtocol",
    "Instruction",
    "InstructionSourceNodeReference",
]
