# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.


from __future__ import annotations


import random
from dataclasses import dataclass
from typing import Callable

from ... import code
from ...estimator import LogicalCounts
from .._qre import Trace
from .._application import Application
from ..interop import trace_from_entry_expr


@dataclass
class OpenQASMApplication(Application[None]):
    """Application that produces a resource estimation trace from OpenQASM code.

    Accepts an OpenQASM program string or a callable.

    Attributes:
        program (str | Callable): The OpenQASM program as string or callable.
        args (tuple): The arguments to pass to the callable, if one is
            provided. Default is an empty tuple.
    """

    program: str | Callable | LogicalCounts
    args: tuple = ()

    def get_trace(self, parameters: None = None) -> Trace:
        """Return the resource estimation trace for the OpenQASM program.

        Args:
            parameters (None): Unused. Defaults to None.

        Returns:
            Trace: The resource estimation trace.
        """
        if isinstance(self.program, str):
            from qsharp.openqasm import import_openqasm, ProgramType

            name_found = False
            for _ in range(1_000):
                name = f"openqasm{random.randint(0, 1_000_000)}"
                if not hasattr(code, "qasm_import") or not hasattr(
                    code.qasm_import, name
                ):
                    name_found = True
                    break

            if not name_found:
                raise RuntimeError(
                    "Failed to find a unique name for the OpenQASM program."
                )

            import_openqasm(self.program, name=name, program_type=ProgramType.File)
            self.program = getattr(code.qasm_import, name)

        return trace_from_entry_expr(self.program, *self.args)
