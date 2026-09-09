"""Audit runner."""

from __future__ import annotations

from collections.abc import Collection, Iterable, Iterator
from dataclasses import dataclass, replace

import qodec as qc

from ._diagnostic import Diagnostic, Phase, Severity
from ._report import Report
from ._rule import Rule, filter_rules


@dataclass(frozen=True)
class _Target:
    artifact: object
    local: str = ""
    where: str = ""

    def locate(self, diagnostic: Diagnostic) -> Diagnostic:
        if self.local and diagnostic.where == self.local:
            return replace(diagnostic, where=self.where)
        return diagnostic


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
        return self._run(qodec, self._targets_for(code, qodec))

    def audit_instruction_set(
        self,
        isa: qc.InstructionSet,
        *,
        qodec: qc.Qodec,
    ) -> Report:
        return self._run(qodec, self._targets_for(isa, qodec))

    def audit_gadget(
        self,
        gadget: qc.Gadget,
        *,
        qodec: qc.Qodec,
    ) -> Report:
        return self._run(qodec, self._targets_for(gadget, qodec))

    def audit_layer(
        self,
        layer: qc.Layer,
        *,
        qodec: qc.Qodec,
    ) -> Report:
        targets = [
            target
            for artifact in (layer, *layer.gadgets.values())
            for target in self._targets_for(artifact, qodec)
        ]
        return self._run(qodec, targets)

    def _run(
        self,
        qodec: qc.Qodec,
        targets: Iterable[_Target],
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
            for _, item in self._run_phase(qodec, target_list, Phase.SEMANTIC, blocked)
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
        targets: list[_Target],
        phase: Phase,
        blocked: Collection[int] = (),
    ) -> Iterator[tuple[_Target, Diagnostic]]:
        for rule in filter_rules(self._rules, phase=phase, disabled=self._disabled):
            for target in targets:
                if id(target) not in blocked and isinstance(
                    target.artifact, rule.target
                ):
                    for diagnostic in rule(target.artifact, qodec=qodec):
                        yield target, target.locate(diagnostic)

    @staticmethod
    def _qodec_targets(qodec: qc.Qodec) -> list[_Target]:
        targets = [_Target(qodec)]
        targets.extend(_Target(code) for code in qodec.codes.values())
        layers = qodec.layers
        for index, layer in enumerate(layers):
            source = layer.instruction_set.name
            context = source
            if index + 1 < len(layers):
                context += f" -> {layers[index + 1].instruction_set.name}"
            targets.append(
                _Target(
                    layer.instruction_set,
                    f"isa[{source!r}]",
                    f"layers[{index}].instruction_set ({source})",
                )
            )
            if index + 1 == len(layers):
                continue
            targets.append(
                _Target(layer, f"layer[{source!r}]", f"layers[{index}] ({context})")
            )
            targets.extend(
                _Target(
                    gadget,
                    f"gadget[{gadget.implements.mnemonic!r}]",
                    f"layers[{index}].gadgets[{mnemonic!r}] ({context})",
                )
                for mnemonic, gadget in layer.gadgets.items()
            )
        return targets

    @classmethod
    def _targets_for(cls, artifact: object, qodec: qc.Qodec) -> list[_Target]:
        return [
            target
            for target in cls._qodec_targets(qodec)
            if target.artifact is artifact
        ] or [_Target(artifact)]


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
