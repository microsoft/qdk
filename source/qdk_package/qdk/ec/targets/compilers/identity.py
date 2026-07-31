"""Identity compiler: pass-through for testing and base cases."""

from __future__ import annotations

from qodec.circuits import Program

from .compiler import CompileResult


class IdentityCompiler:
    """A pass-through compiler. Returns the input program unchanged.

    Useful for testing and for situations where the source program is
    already in the desired target ISA.
    """

    def compile(self, program: Program) -> CompileResult:
        return CompileResult(program=program)
