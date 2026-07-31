"""Audit runner."""

from __future__ import annotations

from collections.abc import Collection, Iterable, Iterator
from dataclasses import replace

import qodec

from .diagnostic import Diagnostic, Phase
from .report import Report
from .rule import Rule, filter_rules
from .severity import Severity


class Auditor:
    def __init__(
        self,
        rules: Iterable[Rule] | None = None,
        *,
        disabled: Collection[str] = (),
        include_informational: bool = False,
        strict: bool = False,
    ) -> None:
        if rules is None:
            from .rules import default_rules

            self._rules = tuple(default_rules())
        else:
            self._rules = tuple(rules)
        self._disabled = frozenset(disabled)
        self._include_informational = include_informational
        self._strict = strict

    @property
    def rules(self) -> tuple[Rule, ...]:
        return self._rules

    def audit(self, codec: qodec.Qodec) -> Report:
        return self._run(codec, self._iter_codec_targets(codec))

    def audit_code(
        self, code: qodec.Code, *, codec: qodec.Qodec | None = None
    ) -> Report:
        return self._run(codec or _placeholder_codec(), [(qodec.Code, code)])

    def audit_instruction_set(
        self,
        isa: qodec.InstructionSet,
        *,
        codec: qodec.Qodec | None = None,
    ) -> Report:
        return self._run(codec or _placeholder_codec(), [(qodec.InstructionSet, isa)])

    def audit_gadget(
        self,
        gadget: qodec.Gadget,
        *,
        codec: qodec.Qodec | None = None,
    ) -> Report:
        return self._run(codec or _placeholder_codec(), [(qodec.Gadget, gadget)])

    def audit_layer(
        self,
        layer: qodec.Layer,
        *,
        codec: qodec.Qodec | None = None,
    ) -> Report:
        targets = [(qodec.Layer, layer)] + [
            (qodec.Gadget, gadget) for gadget in layer.gadgets.values()
        ]
        return self._run(codec or _placeholder_codec(), targets)

    def _run(
        self,
        codec: qodec.Qodec,
        targets: Iterable[tuple[type, object]],
    ) -> Report:
        target_list = list(targets)
        diagnostics = list(self._run_phase(codec, target_list, Phase.STRUCTURAL))
        if not any(item.severity is Severity.ERROR for item in diagnostics):
            diagnostics.extend(self._run_phase(codec, target_list, Phase.SEMANTIC))
        if self._include_informational:
            diagnostics.extend(self._run_phase(codec, target_list, Phase.INFORMATIONAL))
        if self._strict:
            diagnostics = [
                (
                    replace(item, severity=Severity.ERROR)
                    if item.severity is Severity.WARNING
                    else item
                )
                for item in diagnostics
            ]
        return Report(tuple(diagnostics))

    def _run_phase(
        self,
        codec: qodec.Qodec,
        targets: list[tuple[type, object]],
        phase: Phase,
    ) -> Iterator[Diagnostic]:
        for rule in filter_rules(self._rules, phase=phase, disabled=self._disabled):
            for target_type, target in targets:
                if rule.target is target_type:
                    yield from rule(target, codec=codec)

    @staticmethod
    def _iter_codec_targets(
        codec: qodec.Qodec,
    ) -> list[tuple[type, object]]:
        targets: list[tuple[type, object]] = [(qodec.Qodec, codec)]
        targets.extend(
            (qodec.InstructionSet, isa) for isa in codec.instruction_sets.values()
        )
        targets.extend((qodec.Code, code) for code in codec.codes.values())
        for layer in codec.layers[:-1]:
            targets.append((qodec.Layer, layer))
            targets.extend((qodec.Gadget, gadget) for gadget in layer.gadgets.values())
        return targets


def audit(codec: qodec.Qodec, **kwargs: object) -> Report:
    return Auditor(**kwargs).audit(codec)  # type: ignore[arg-type]


def _placeholder_codec() -> qodec.Qodec:
    return qodec.Qodec(
        layers=[qodec.Layer(qodec.InstructionSet("_placeholder"))],
        name="_placeholder",
    )


__all__ = ["Auditor", "audit"]
