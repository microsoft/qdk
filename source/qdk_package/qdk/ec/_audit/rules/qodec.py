"""Whole-qodec audit rules."""

from collections.abc import Iterator
from dataclasses import dataclass

import qodec as qc

from .._diagnostic import Diagnostic, Phase, Severity
from .._rule import Rule
from .._structure import structural_issues


@dataclass(frozen=True)
class StructuralValidationRule:
    name: str = "qodec/invalid-structure"
    severity: Severity = Severity.ERROR
    phase: Phase = Phase.STRUCTURAL
    target: type = object

    def __call__(self, target: object, *, qodec: qc.Qodec) -> Iterator[Diagnostic]:
        if not isinstance(target, (qc.Qodec, qc.Code, qc.InstructionSet, qc.Gadget)):
            return
        for path, message in structural_issues(target):
            yield Diagnostic(self.name, self.severity, message, path)


@dataclass(frozen=True)
class MissingRealizationRule:
    name: str = "gadget/missing-realization"
    severity: Severity = Severity.INFO
    phase: Phase = Phase.INFORMATIONAL
    target: type = qc.Qodec

    def __call__(self, target: object, *, qodec: qc.Qodec) -> Iterator[Diagnostic]:
        if not isinstance(target, qc.Qodec):
            raise TypeError(f"expected qodec.Qodec, got {type(target).__name__}")
        for index, layer in enumerate(target.layers[:-1]):
            for mnemonic in layer.instruction_set.instructions:
                if mnemonic not in layer.gadgets:
                    yield Diagnostic(
                        self.name,
                        self.severity,
                        f"No explicit gadget for instruction {mnemonic!r}",
                        f"layers[{index}].instruction_set.instructions[{mnemonic!r}] "
                        f"({layer.instruction_set.name} -> {target.layers[index + 1].instruction_set.name})",
                        f"No entry: layers[{index}].gadgets[{mnemonic!r}]",
                    )


RULES: tuple[Rule, ...] = (
    StructuralValidationRule(),
    MissingRealizationRule(),
)

__all__ = ["MissingRealizationRule", "StructuralValidationRule", "RULES"]
