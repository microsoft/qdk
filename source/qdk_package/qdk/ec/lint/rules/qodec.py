"""Whole-qodec audit rules."""

from collections.abc import Iterator
from dataclasses import dataclass

import qodec

from ...lint._diagnostic import Diagnostic, Phase
from ...lint._rule import Rule
from ...lint._severity import Severity


@dataclass(frozen=True)
class MissingSourceInstructionRule:
    name: str = "gadget/missing-source-instruction"
    severity: Severity = Severity.ERROR
    phase: Phase = Phase.STRUCTURAL
    target: type = qodec.Qodec

    def __call__(self, target: object, *, codec: qodec.Qodec) -> Iterator[Diagnostic]:
        if not isinstance(target, qodec.Qodec):
            raise TypeError(f"expected qodec.Qodec, got {type(target).__name__}")
        for index, layer in enumerate(target.layers):
            source = set(layer.isa.instructions)
            for mnemonic in layer.gadgets:
                if mnemonic not in source:
                    yield Diagnostic(
                        self.name,
                        self.severity,
                        f"gadget keyed {mnemonic!r} has no matching instruction "
                        f"in source ISA {layer.isa.name!r}",
                        f"layers[{index}].gadgets[{mnemonic!r}]",
                    )


@dataclass(frozen=True)
class MissingRealizationRule:
    name: str = "gadget/missing-realization"
    severity: Severity = Severity.ERROR
    phase: Phase = Phase.STRUCTURAL
    target: type = qodec.Qodec

    def __call__(self, target: object, *, codec: qodec.Qodec) -> Iterator[Diagnostic]:
        if not isinstance(target, qodec.Qodec):
            raise TypeError(f"expected qodec.Qodec, got {type(target).__name__}")
        for index, layer in enumerate(target.layers[:-1]):
            for mnemonic in layer.isa.instructions:
                if mnemonic not in layer.gadgets:
                    yield Diagnostic(
                        self.name,
                        self.severity,
                        f"instruction {mnemonic!r} of ISA {layer.isa.name!r} "
                        f"has no gadget in layer {index}",
                        f"layers[{index}]",
                    )


RULES: tuple[Rule, ...] = (
    MissingSourceInstructionRule(),
    MissingRealizationRule(),
)

__all__ = ["MissingRealizationRule", "MissingSourceInstructionRule", "RULES"]
