"""Audit runner."""

from __future__ import annotations

from collections.abc import Collection, Iterable, Iterator
from dataclasses import dataclass, replace
import json

import qodec as qc

from ._diagnostic import Diagnostic, Phase, Severity
from ._report import Report
from ._rule import Rule, filter_rules


@dataclass(frozen=True)
class _Target:
    artifact: object
    local: str = ""
    where: str = ""
    path: str = ""
    dependencies: frozenset[str | int] = frozenset()

    def locate(self, diagnostic: Diagnostic) -> Diagnostic:
        if self.where and diagnostic.where in (self.local, self.path):
            return replace(diagnostic, where=self.where)
        return diagnostic

    @property
    def key(self) -> str | int:
        return (
            self.path
            if self.path or isinstance(self.artifact, qc.Qodec)
            else id(self.artifact)
        )


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
        context = self._qodec_targets(qodec)
        relevant = {target.path for target in target_list if target.path}
        relevant.update(
            path
            for target in target_list
            for path in target.dependencies
            if isinstance(path, str)
        )
        selected = {target.key for target in target_list}
        target_list.extend(
            target
            for target in context
            if target.path in relevant and target.key not in selected
        )
        located = {target.path: target for target in context}
        located.update({target.path: target for target in target_list if target.path})
        whole_model = any(
            isinstance(target.artifact, qc.Qodec) for target in target_list
        )
        structural_targets = target_list
        if relevant and not whole_model:
            structural_targets = [context[0], *target_list]
        diagnostics: list[Diagnostic] = []
        blocked: set[str | int] = set()
        for target, item in self._run_phase(
            qodec, structural_targets, Phase.STRUCTURAL
        ):
            if (
                isinstance(target.artifact, qc.Qodec)
                and item.rule == "qodec/invalid-structure"
            ):
                if item.where and not whole_model and item.where not in relevant:
                    continue
                target = located.get(item.where, target)
            diagnostic = self._apply_policy(target.locate(item))
            diagnostics.append(diagnostic)
            if diagnostic.severity is Severity.ERROR:
                blocked.add(target.key)
        target_list.sort(
            key=lambda target: (
                0 if isinstance(target.artifact, (qc.Code, qc.InstructionSet)) else 1
            )
        )
        for target, item in self._run_phase(
            qodec, target_list, Phase.SEMANTIC, blocked
        ):
            diagnostic = self._apply_policy(item)
            diagnostics.append(diagnostic)
            if diagnostic.severity is Severity.ERROR and (
                isinstance(target.artifact, (qc.Code, qc.InstructionSet))
                or item.rule == "instruction-set/invalid-clifford"
            ):
                blocked.add(target.key)
        if self._include_informational:
            diagnostics.extend(
                self._apply_policy(item)
                for _, item in self._run_phase(
                    qodec, target_list, Phase.INFORMATIONAL, blocked
                )
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
        blocked: Collection[str | int] = (),
    ) -> Iterator[tuple[_Target, Diagnostic]]:
        rules = filter_rules(self._rules, phase=phase, disabled=self._disabled)
        for target in targets:
            for rule in rules:
                if rule.name == "qodec/invalid-structure" and target.path:
                    continue
                if (
                    rule.name == "instruction-set/invalid-clifford"
                    and target.path
                    and isinstance(target.artifact, qc.Gadget)
                ):
                    continue
                if not (
                    "" in blocked
                    or target.key in blocked
                    or any(path in blocked for path in target.dependencies)
                ) and isinstance(target.artifact, rule.target):
                    for diagnostic in rule(target.artifact, qodec=qodec):
                        yield target, target.locate(diagnostic)

    @staticmethod
    def _qodec_targets(qodec: qc.Qodec) -> list[_Target]:
        targets = [_Target(qodec, where="qodec")]
        targets.extend(
            _Target(
                code,
                f"code[{code.name!r}]",
                f"codes[{code.name!r}]",
                f"codes[{json.dumps(code.name, ensure_ascii=False)}]",
            )
            for code in qodec.codes.values()
        )
        layers = qodec.layers
        instruction_set_paths: dict[str, frozenset[str | int]] = {}
        for index, layer in enumerate(layers):
            name = layer.instruction_set.name
            instruction_set_paths[name] = instruction_set_paths.get(
                name, frozenset()
            ) | {f"layers[{index}].instruction_set"}
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
                    f"layers[{index}].instruction_set",
                    instruction_set_paths[source],
                )
            )
            if index + 1 == len(layers):
                continue
            targets.append(
                _Target(
                    layer,
                    f"layer[{source!r}]",
                    f"layers[{index}] ({context})",
                    f"layers[{index}]",
                )
            )
            targets.extend(
                _Target(
                    gadget,
                    f"gadget[{gadget.implements.mnemonic!r}]",
                    f"layers[{index}].gadgets[{mnemonic!r}] ({context})",
                    f"layers[{index}].gadgets[{json.dumps(mnemonic, ensure_ascii=False)}]",
                    instruction_set_paths[source]
                    | instruction_set_paths[layers[index + 1].instruction_set.name]
                    | frozenset(
                        f"codes[{json.dumps(encoding.code.name, ensure_ascii=False)}]"
                        for encoding in (*gadget.inputs, *gadget.outputs)
                    ),
                )
                for mnemonic, gadget in layer.gadgets.items()
            )
        return targets

    @classmethod
    def _targets_for(cls, artifact: object, qodec: qc.Qodec) -> list[_Target]:
        attached = [
            target
            for target in cls._qodec_targets(qodec)
            if target.artifact is artifact
        ]
        if attached:
            return attached
        if isinstance(artifact, qc.Gadget):
            prerequisites = [
                target
                for dependency in (
                    artifact.circuit.instruction_set,
                    *(
                        encoding.code
                        for encoding in (*artifact.inputs, *artifact.outputs)
                    ),
                )
                for target in cls._targets_for(dependency, qodec)
            ]
            prerequisites = list(
                {target.key: target for target in prerequisites}.values()
            )
            local = f"gadget[{artifact.implements.mnemonic!r}]"
            return [
                *prerequisites,
                _Target(
                    artifact,
                    local,
                    local,
                    dependencies=frozenset(target.key for target in prerequisites),
                ),
            ]
        if isinstance(artifact, qc.Code):
            local = f"code[{artifact.name!r}]"
            return [_Target(artifact, local, local)]
        if isinstance(artifact, qc.InstructionSet):
            local = f"isa[{artifact.name!r}]"
            return [_Target(artifact, local, local)]
        return [_Target(artifact)]


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
