# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations

from abc import ABC, abstractmethod
from dataclasses import dataclass, field

from ._qre import ISA


class Architecture(ABC):
    @property
    @abstractmethod
    def provided_isa(self) -> ISA: ...

    def context(self) -> _Context:
        """Create a new enumeration context for this architecture."""
        return _Context(self.provided_isa)


@dataclass(slots=True, frozen=True)
class _Context:
    """
    Context passed through enumeration, holding shared state.

    Attributes:
        root_isa: The root ISA for enumeration.
    """

    root_isa: ISA
    _bindings: dict[str, ISA] = field(default_factory=dict, repr=False)

    def _with_binding(self, name: str, isa: ISA) -> _Context:
        """Return a new context with an additional binding (internal use)."""
        new_bindings = {**self._bindings, name: isa}
        return _Context(self.root_isa, new_bindings)
