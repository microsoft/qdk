# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from abc import ABC, abstractmethod

from ._qre import ISA


class Architecture(ABC):
    @property
    @abstractmethod
    def provided_isa(self) -> ISA: ...
