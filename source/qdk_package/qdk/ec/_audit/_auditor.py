"""Audit runner."""

from __future__ import annotations

from collections.abc import Collection, Iterable, Iterator
from dataclasses import replace

import qodec as qc

from ._diagnostic import Diagnostic, Phase, Severity
from ._report import Report
from ._rule import Rule, filter_rules


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
        return self._run(qodec, self._qodec_targets(qodec))

    def audit_code(self, code: qc.Code, *, qodec: qc.Qodec) -> Report:
        return self._run(qodec, [code])

    def audit_instruction_set(
        self,
        isa: qc.InstructionSet,
        *,
        qodec: qc.Qodec,
    ) -> Report:
        return self._run(qodec, [isa])

    def audit_gadget(
        self,
        gadget: qc.Gadget,
        *,
        qodec: qc.Qodec,
    ) -> Report:
        return self._run(qodec, [gadget])

    def audit_layer(
        self,
        layer: qc.Layer,
        *,
        qodec: qc.Qodec,
    ) -> Report:
        targets = [layer, *layer.gadgets.values()]
        return self._run(qodec, targets)

    def _run(
        self,
        qodec: qc.Qodec,
        targets: Iterable[object],
    ) -> Report:
        target_list = list(targets)
        diagnostics: list[Diagnostic] = []
        blocked: set[int] = set()
        for target, item in self._run_phase(qodec, target_list, Phase.STRUCTURAL):
            diagnostic = self._apply_policy(item)
            diagnostics.append(diagnostic)
            if diagnostic.severity is Severity.ERROR:
                blocked.add(id(target))
        diagnostics.extend(
            self._apply_policy(item)
            for target, item in self._run_phase(qodec, target_list, Phase.SEMANTIC)
            if id(target) not in blocked
        )
        if self._include_informational:
            diagnostics.extend(
                self._apply_policy(item)
                for _, item in self._run_phase(qodec, target_list, Phase.INFORMATIONAL)
            )
        return Report(tuple(diagnostics))

    def _apply_policy(self, diagnostic: Diagnostic) -> Diagnostic:
        if self._strict and diagnostic.severity is Severity.WARNING:
            return replace(diagnostic, severity=Severity.ERROR)
        return diagnostic

    def _run_phase(
        self,
        qodec: qc.Qodec,
        targets: list[object],
        phase: Phase,
    ) -> Iterator[tuple[object, Diagnostic]]:
        for rule in filter_rules(self._rules, phase=phase, disabled=self._disabled):
            for target in targets:
                if isinstance(target, rule.target):
                    for diagnostic in rule(target, qodec=qodec):
                        yield target, diagnostic

    @staticmethod
    def _qodec_targets(qodec: qc.Qodec) -> list[object]:
        targets: list[object] = [qodec]
        targets.extend(qodec.instruction_sets.values())
        targets.extend(qodec.codes.values())
        for layer in qodec.layers[:-1]:
            targets.append(layer)
            targets.extend(layer.gadgets.values())
        return targets


def audit(
    qodec: qc.Qodec,
    *,
    disabled: Collection[str] = (),
    promote_warnings: bool = False,
) -> Report:
    """Run every enabled audit rule over a whole qodec.

    The returned report carries every diagnostic the rules produced, including
    informational ones; filtering is the caller's to do on read.
    ``promote_warnings`` reclassifies warnings as errors, it does not filter.
    """
    return Auditor(
        disabled=disabled,
        include_informational=True,
        strict=promote_warnings,
    ).audit(qodec)


__all__ = ["Auditor", "audit"]
