"""Instruction-set audit rules."""

from collections.abc import Iterator
from dataclasses import dataclass

import qodec as qc

from ..._analysis.propagation.isa_actions import build_clifford_images
from ..._analysis.propagation.pauli import parse_term
from .._diagnostic import Diagnostic, Phase, Severity
from .._rule import Rule


@dataclass(frozen=True)
class UnreferencedBlockRule:
    name: str = "instruction-set/unreferenced-block"
    severity: Severity = Severity.INFO
    phase: Phase = Phase.INFORMATIONAL
    target: type = qc.InstructionSet

    def __call__(self, target: object, *, qodec: qc.Qodec) -> Iterator[Diagnostic]:
        if not isinstance(target, qc.InstructionSet):
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
                    f"block type {block.name!r} is unused by instruction inputs and outputs",
                    f"isa[{target.name!r}]",
                    f"Declared capacity: {block.encodes} qubit(s).",
                )


@dataclass(frozen=True)
class CliffordAlgebraRule:
    name: str = "instruction-set/invalid-clifford"
    severity: Severity = Severity.ERROR
    phase: Phase = Phase.SEMANTIC
    target: type = object

    def __call__(self, target: object, *, qodec: qc.Qodec) -> Iterator[Diagnostic]:
        if isinstance(target, qc.InstructionSet):
            instructions = list(target.instructions.values())
            where = f"isa[{target.name!r}]"
        elif isinstance(target, qc.Gadget):
            instructions = [target.implements]
            where = f"gadget[{target.implements.mnemonic!r}]"
        else:
            return
        for instruction in instructions:
            for position, action in enumerate(instruction.action):
                if not isinstance(action, qc.actions.Clifford):
                    continue
                try:
                    support = sorted(
                        {
                            parse_term(token)[1]
                            for pair in action.generators.items()
                            for text in pair
                            for token in text.split()
                        }
                    )
                    qubit_map = {index: index for index in support}
                    local_map = {index: local for local, index in enumerate(support)}
                    build_clifford_images(
                        action.generators, qubit_map, local_map, len(support)
                    )
                except NotImplementedError as error:
                    yield Diagnostic(
                        self.name,
                        Severity.WARNING,
                        f"instruction {instruction.mnemonic!r} action[{position}]: Clifford algebra not checked",
                        where,
                        str(error),
                    )
                except (KeyError, ValueError) as error:
                    yield Diagnostic(
                        self.name,
                        self.severity,
                        f"instruction {instruction.mnemonic!r} action[{position}]: invalid Clifford map",
                        where,
                        str(error),
                    )


RULES: tuple[Rule, ...] = (CliffordAlgebraRule(), UnreferencedBlockRule())

__all__ = ["CliffordAlgebraRule", "RULES", "UnreferencedBlockRule"]
