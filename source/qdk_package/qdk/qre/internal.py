# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Internal types that appear in the ``qdk.qre`` public API surface.

.. warning::
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
    from typing import Optional, Protocol, Union

    from ._architecture import Architecture
    from ._instruction import ISATransform

    # ------------------------------------------------------------------
    # ApplicationContext
    #   (runtime: _application.ApplicationContext)
    # ------------------------------------------------------------------
    class ApplicationContext(Protocol):
        """Enumeration context wrapping an application instance.

        Obtained via :meth:`Application.context` and passed to
        :meth:`TraceQuery.enumerate`.
        """

        @property
        def application(self) -> "Application": ...

    # ------------------------------------------------------------------
    # InstructionSourceNodeReference
    #   (runtime: _instruction._InstructionSourceNodeReference)
    # ------------------------------------------------------------------
    class InstructionSourceNodeReference(Protocol):
        """Reference to a node in an ``InstructionSource`` graph."""

        @property
        def instruction(self) -> Instruction: ...
        @property
        def transform(self) -> Optional[Union[ISATransform, Architecture]]: ...

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
        def arity(self) -> Optional[int]: ...
        def space(self, arity: Optional[int] = None) -> Optional[int]: ...
        def time(self, arity: Optional[int] = None) -> Optional[int]: ...
        def error_rate(self, arity: Optional[int] = None) -> Optional[float]: ...
        def expect_time(self, arity: Optional[int] = None) -> int: ...
        def expect_error_rate(self, arity: Optional[int] = None) -> float: ...

else:
    from ._application import ApplicationContext
    from ._instruction import (
        _InstructionSourceNodeReference as InstructionSourceNodeReference,
    )
    from ._qre import Instruction

__all__ = [
    "ApplicationContext",
    "Instruction",
    "InstructionSourceNodeReference",
]
