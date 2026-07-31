"""Compiler protocol and result type."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Protocol, runtime_checkable

from qodec.circuits import Program


@dataclass
class CompileResult:
    """Output of a compiler.

    ``program`` is the lowered `Program`. Future fields (operand maps,
    outcome maps) will be added here as targets prove they need them.
    """

    program: Program


@runtime_checkable
class Compiler(Protocol):
    """Lower a `Program` from one ISA to another."""

    def compile(self, program: Program) -> CompileResult: ...
