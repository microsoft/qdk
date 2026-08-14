"""Audit runner."""

from __future__ import annotations

from collections.abc import Collection, Iterable, Iterator
from dataclasses import replace

import qodec as qc

from ._diagnostic import Diagnostic, Phase
from ._report import Report
from ._rule import Rule, filter_rules
from ._severity import Severity


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

    def audit(self, qodec: qc.Qodec) -> Report:
        return self._run(qodec, self._iter_qodec_targets(qodec))

    def audit_code(
        self, code: qc.Code, *, qodec: qc.Qodec | None = None
    ) -> Report:
        return self._run(qodec or _placeholder_qodec(), [(qc.Code, code)])

    def audit_instruction_set(
        self,
        isa: qc.InstructionSet,
        *,
        qodec: qc.Qodec | None = None,
    ) -> Report:
        return self._run(qodec or _placeholder_qodec(), [(qc.InstructionSet, isa)])

    def audit_gadget(
        self,
        gadget: qc.Gadget,
        *,
        qodec: qc.Qodec | None = None,
    ) -> Report:
        return self._run(qodec or _placeholder_qodec(), [(qc.Gadget, gadget)])

    def audit_layer(
        self,
        layer: qc.Layer,
        *,
        qodec: qc.Qodec | None = None,
    ) -> Report:
        targets = [(qc.Layer, layer)] + [
            (qc.Gadget, gadget) for gadget in layer.gadgets.values()
        ]
        return self._run(qodec or _placeholder_qodec(), targets)

    def _run(
        self,
        qodec: qc.Qodec,
        targets: Iterable[tuple[type, object]],
    ) -> Report:
        target_list = list(targets)
        diagnostics = list(self._run_phase(qodec, target_list, Phase.STRUCTURAL))
        if not any(item.severity is Severity.ERROR for item in diagnostics):
            diagnostics.extend(self._run_phase(qodec, target_list, Phase.SEMANTIC))
        if self._include_informational:
            diagnostics.extend(self._run_phase(qodec, target_list, Phase.INFORMATIONAL))
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
        qodec: qc.Qodec,
        targets: list[tuple[type, object]],
        phase: Phase,
    ) -> Iterator[Diagnostic]:
        for rule in filter_rules(self._rules, phase=phase, disabled=self._disabled):
            for target_type, target in targets:
                if rule.target is target_type:
                    yield from rule(target, qodec=qodec)

    @staticmethod
    def _iter_qodec_targets(
        qodec: qc.Qodec,
    ) -> list[tuple[type, object]]:
        targets: list[tuple[type, object]] = [(qc.Qodec, qodec)]
        targets.extend(
            (qc.InstructionSet, isa) for isa in qodec.instruction_sets.values()
        )
        targets.extend((qc.Code, code) for code in qodec.codes.values())
        for layer in qodec.layers[:-1]:
            targets.append((qc.Layer, layer))
            targets.extend((qc.Gadget, gadget) for gadget in layer.gadgets.values())
        return targets


def audit(qodec: qc.Qodec, **kwargs: object) -> Report:
    return Auditor(**kwargs).audit(qodec)  # type: ignore[arg-type]


def _placeholder_qodec() -> qc.Qodec:
    return qc.Qodec(
        layers=[qc.Layer(qc.InstructionSet("_placeholder"))],
        name="_placeholder",
    )


__all__ = ["Auditor", "audit"]
