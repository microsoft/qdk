"""Instruction-set audit rules."""

from collections.abc import Iterator
from dataclasses import dataclass

import qodec

from ..diagnostic import Diagnostic, Phase
from ..rule import Rule
from ..severity import Severity


@dataclass(frozen=True)
class UnreferencedBlockRule:
    name: str = "isa/unreferenced-block"
    severity: Severity = Severity.INFO
    phase: Phase = Phase.INFORMATIONAL
    target: type = qodec.InstructionSet

    def __call__(self, target: object, *, codec: qodec.Qodec) -> Iterator[Diagnostic]:
        if not isinstance(target, qodec.InstructionSet):
            raise TypeError(
                f"expected qodec.InstructionSet, got {type(target).__name__}"
            )
        referenced = {
            operand.block
            for instruction in target.instructions.values()
            for operand in (*instruction.inputs, *instruction.outputs)
        }
        if not referenced:
            return
        for block in target.blocks:
            if block.name not in referenced:
                yield Diagnostic(
                    self.name,
                    self.severity,
                    f"block type {block.name!r} is declared but not referenced "
                    "by any instruction operand",
                    f"isa[{target.name!r}]",
                )


RULES: tuple[Rule, ...] = (UnreferencedBlockRule(),)

__all__ = ["RULES", "UnreferencedBlockRule"]
