# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Internal types that appear in the public API surface.

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
    from enum import Enum
    from typing import Any, Dict, Optional, Protocol, Union

    class StateDumpData(Protocol):
        """A state dump returned from the Q# interpreter."""

        def __repr__(self) -> str: ...
        def __str__(self) -> str: ...
        def _repr_markdown_(self) -> str: ...
        def _repr_latex_(self) -> Optional[str]: ...

    class Circuit(Protocol):
        """A quantum circuit diagram generated from a Q# or OpenQASM program."""

        def json(self) -> str: ...
        def __repr__(self) -> str: ...
        def __str__(self) -> str: ...

    class Closure(Protocol):
        """An opaque closure reference that can be passed back into Q#."""

        ...

    class GlobalCallable(Protocol):
        """An opaque callable reference that can be passed back into Q#."""

        ...

    class Output(Protocol):
        """An output returned from the Q# interpreter.

        Outputs can be state dumps, matrices, or messages.
        """

        def __repr__(self) -> str: ...
        def __str__(self) -> str: ...
        def _repr_markdown_(self) -> Optional[str]: ...

    class Config(Protocol):
        """Configuration hints for the language service."""

        def __repr__(self) -> str: ...
        def _repr_mimebundle_(
            self,
            include: Union[Any, None] = None,
            exclude: Union[Any, None] = None,
        ) -> Dict[str, Dict[str, Any]]: ...

    class QirInputData(Protocol):
        """Wraps a compiled QIR program for submission to a quantum target.

        Implements the ``QirRepresentable`` protocol expected by the
        ``azure-quantum`` package.
        """

        _name: str

        def _repr_qir_(self, **kwargs) -> bytes: ...
        def __str__(self) -> str: ...

    class ZoneType(Enum):
        """Type of zone in a neutral-atom device layout."""

        REG = "register"
        INTER = "interaction"
        MEAS = "measurement"

    class Zone(Protocol):
        """A zone in a neutral-atom device layout."""

        name: str
        row_count: int
        type: ZoneType
        offset: int

        def set_offset(self, offset: int) -> None: ...

else:
    from ._native import (  # type: ignore
        Circuit,
        Closure,
        GlobalCallable,
        Output,
        StateDumpData,
    )
    from ._types import (
        Config,
        QirInputData,
    )
    from ._device._device import Zone, ZoneType

__all__ = [
    "Circuit",
    "Closure",
    "Config",
    "GlobalCallable",
    "Output",
    "QirInputData",
    "StateDumpData",
    "Zone",
    "ZoneType",
]
