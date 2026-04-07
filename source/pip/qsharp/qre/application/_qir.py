# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.


from __future__ import annotations

from dataclasses import dataclass

from .._qre import Trace
from .._application import Application
from ..interop import trace_from_qir


@dataclass
class QIRApplication(Application[None]):
    """Application that produces a resource estimation trace from QIR code.

    Accepts QIR input as LLVM IR text or bitcode.

    Attributes:
        input (str | bytes): QIR input as LLVM IR text (str) or
            bitcode (bytes).
    """

    input: str | bytes

    def get_trace(self, parameters: None = None) -> Trace:
        """Return the resource estimation trace for the QIR program.

        Args:
            parameters (None): Unused. Defaults to None.

        Returns:
            Trace: The resource estimation trace.
        """
        return trace_from_qir(self.input)
