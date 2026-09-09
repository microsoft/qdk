"""Whole-qodec audit rules."""

from collections.abc import Iterator
from dataclasses import dataclass

import qodec as qc

from .._diagnostic import Diagnostic, Phase, Severity
from .._rule import Rule


@dataclass(frozen=True)
class MissingSourceInstructionRule:
    name: str = "gadget/missing-source-instruction"
    severity: Severity = Severity.ERROR
    phase: Phase = Phase.STRUCTURAL
    target: type = qc.Qodec

    def __call__(self, target: object, *, qodec: qc.Qodec) -> Iterator[Diagnostic]:
        if not isinstance(target, qc.Qodec):
            raise TypeError(f"expected qodec.Qodec, got {type(target).__name__}")
        for index, layer in enumerate(target.layers):
            source = set(layer.instruction_set.instructions)
            for mnemonic in layer.gadgets:
                if mnemonic not in source:
                    yield Diagnostic(
                        self.name,
                        self.severity,
                        f"implements: no source instruction named {mnemonic!r}",
                        f"layers[{index}].gadgets[{mnemonic!r}] "
                        f"({layer.instruction_set.name}"
                        + (
                            f" -> {target.layers[index + 1].instruction_set.name}"
                            if index + 1 < len(target.layers)
                            else ""
                        )
                        + ")",
                        f"Source: layers[{index}].instruction_set.instructions",
                    )


@dataclass(frozen=True)
class MissingRealizationRule:
    name: str = "gadget/missing-realization"
    severity: Severity = Severity.ERROR
    phase: Phase = Phase.STRUCTURAL
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
                        f"No gadget implements instruction {mnemonic!r}",
                        f"layers[{index}].instruction_set.instructions[{mnemonic!r}] "
                        f"({layer.instruction_set.name} -> {target.layers[index + 1].instruction_set.name})",
                        f"Missing: layers[{index}].gadgets[{mnemonic!r}]",
                    )


RULES: tuple[Rule, ...] = (
    MissingSourceInstructionRule(),
    MissingRealizationRule(),
)

__all__ = ["MissingRealizationRule", "MissingSourceInstructionRule", "RULES"]
