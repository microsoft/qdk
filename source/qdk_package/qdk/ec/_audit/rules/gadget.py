"""Per-gadget audit rules."""

from __future__ import annotations

from collections.abc import Iterable, Iterator
from dataclasses import dataclass
import json

import qodec as qc

from ..._readouts import flag_slots, observable_slots, observe_count_of
from ..._references import StabilizerSign
from ..._analysis.channel_action import (
    declared_action_of,
    realized_action_of,
)
from ..._analysis.check_discovery import _output_relations_of
from ..._analysis.declaration_issues import declaration_issues
from .._diagnostic import Diagnostic, Phase, Severity
from .._parity import ParityAnalysis
from .._readout_check import readout_disagreements
from .._rule import Rule


def _where(gadget: qc.Gadget) -> str:
    return f"gadget[{gadget.implements.mnemonic!r}]"


def _gadget(target: object) -> qc.Gadget:
    if not isinstance(target, qc.Gadget):
        raise TypeError(f"expected qodec.Gadget, got {type(target).__name__}")
    return target


def _equation(terms: Iterable[object]) -> str:
    return json.dumps([str(term) for term in terms])


def _observable(gadget: qc.Gadget, position: int) -> str:
    return str(
        [
            observable
            for action in gadget.implements.action
            if isinstance(action, qc.actions.Observe)
            for observable in action.observables
        ][position]
    )


@dataclass(frozen=True)
class MissingCheckRule:
    name: str = "gadget/missing-check"
    severity: Severity = Severity.INFO
    phase: Phase = Phase.INFORMATIONAL
    target: type = qc.Gadget

    def __call__(self, target: object, *, qodec: qc.Qodec) -> Iterator[Diagnostic]:
        gadget = _gadget(target)
        try:
            missing = ParityAnalysis(gadget).missing_checks()
        except (KeyError, ValueError, TypeError, NotImplementedError) as error:
            yield Diagnostic(
                self.name,
                self.severity,
                "Check completeness not checked: analysis unavailable",
                _where(gadget),
                f"{type(error).__name__}: {error}",
            )
            return
        for equation in missing:
            yield Diagnostic(
                self.name,
                self.severity,
                "An independent measurement check is undeclared",
                _where(gadget),
                f"Verified relation: {_equation(equation)}\n"
                "Not implied by the declared checks, readout definitions, or zero-valued flags.",
            )


@dataclass(frozen=True)
class MissingObservableRule:
    name: str = "gadget/missing-observable"
    severity: Severity = Severity.INFO
    phase: Phase = Phase.INFORMATIONAL
    target: type = qc.Gadget

    def __call__(self, target: object, *, qodec: qc.Qodec) -> Iterator[Diagnostic]:
        gadget = _gadget(target)
        for missing in declaration_issues(gadget).missing_observables:
            yield Diagnostic(
                self.name,
                self.severity,
                f"readouts[{missing}] is unbound (logical {_observable(gadget, int(missing))})",
                _where(gadget),
                f"Expected: {observe_count_of(gadget.implements)} observable bindings; "
                f"declared: {len(observable_slots(gadget))}.",
            )


@dataclass(frozen=True)
class MissingFlagRule:
    name: str = "gadget/missing-flag"
    severity: Severity = Severity.INFO
    phase: Phase = Phase.INFORMATIONAL
    target: type = qc.Gadget

    def __call__(self, target: object, *, qodec: qc.Qodec) -> Iterator[Diagnostic]:
        gadget = _gadget(target)
        for missing in declaration_issues(gadget).missing_flags:
            position = observe_count_of(gadget.implements) + list(
                gadget.implements.flags
            ).index(missing)
            yield Diagnostic(
                self.name,
                self.severity,
                f"readouts[{position}] is unbound (flag {missing!r})",
                _where(gadget),
                f"Expected: {len(gadget.implements.flags)} flag bindings; "
                f"declared: {len(flag_slots(gadget))}.",
            )


@dataclass(frozen=True)
class UnsupportedActionAtomRule:
    name: str = "gadget/unsupported-action-atom"
    severity: Severity = Severity.WARNING
    phase: Phase = Phase.SEMANTIC
    target: type = qc.Gadget

    def __call__(self, target: object, *, qodec: qc.Qodec) -> Iterator[Diagnostic]:
        gadget = _gadget(target)
        unsupported = set(declaration_issues(gadget).unsupported_atoms)
        for index, action in enumerate(gadget.implements.action):
            atom_name = type(action).__name__
            if atom_name not in unsupported:
                continue
            if (
                isinstance(action, (qc.actions.Pauli, qc.actions.Clifford))
                and action.condition is None
            ):
                continue
            yield Diagnostic(
                self.name,
                self.severity,
                f"implements.action[{index}] ({atom_name}) is not supported by the action verifier",
                _where(gadget),
                "Logical action not checked.",
            )


@dataclass(frozen=True)
class CheckMismatchRule:
    name: str = "gadget/check-mismatch"
    severity: Severity = Severity.ERROR
    phase: Phase = Phase.SEMANTIC
    target: type = qc.Gadget

    def __call__(self, target: object, *, qodec: qc.Qodec) -> Iterator[Diagnostic]:
        gadget = _gadget(target)
        analysis = ParityAnalysis(gadget)
        for index, equation in enumerate(analysis.checks):
            yield from _zero_parity_diagnostic(
                self.name,
                gadget,
                analysis,
                equation,
                f"checks[{index}]",
                _equation(gadget.checks[index]),
            )


def _zero_parity_diagnostic(
    rule: str,
    gadget: qc.Gadget,
    analysis: ParityAnalysis,
    equation: tuple[str, ...],
    label: str,
    declared: str,
) -> Iterator[Diagnostic]:
    if not equation:
        return
    try:
        value = analysis.value(equation)
        if value.is_zero:
            return
        if not any(value[index] for index in range(len(value) - 1)):
            evidence = "Parity: 1; expected: 0."
        else:
            evidence = f"Parity can be 1; expected: 0.\nWitness: {analysis.witness(equation, value)}"
    except (KeyError, ValueError, TypeError, NotImplementedError) as error:
        yield Diagnostic(
            rule,
            Severity.WARNING,
            f"{label}: parity not checked",
            _where(gadget),
            f"Declared: {declared}\n{type(error).__name__}: {error}",
        )
        return
    yield Diagnostic(
        rule,
        Severity.ERROR,
        f"{label}: nonzero parity on noiseless execution",
        _where(gadget),
        f"Declared: {declared}\n{evidence}",
    )


@dataclass(frozen=True)
class FlagMismatchRule:
    name: str = "gadget/flag-mismatch"
    severity: Severity = Severity.ERROR
    phase: Phase = Phase.SEMANTIC
    target: type = qc.Gadget

    def __call__(self, target: object, *, qodec: qc.Qodec) -> Iterator[Diagnostic]:
        gadget = _gadget(target)
        analysis = ParityAnalysis(gadget)
        slots = [
            slot for slot in flag_slots(gadget) if analysis.readouts[slot.position]
        ]
        if not slots:
            return
        try:
            values, unresolved, error = analysis.resolved
        except (KeyError, ValueError, TypeError, NotImplementedError) as error:
            yield Diagnostic(
                self.name,
                Severity.WARNING,
                "Flags not checked: analysis failed",
                _where(gadget),
                f"{type(error).__name__}: {error}",
            )
            return
        del values
        for slot in slots:
            label = f"readouts[{slot.position}] (flag {slot.name!r})"
            if error or slot.position in unresolved:
                yield Diagnostic(
                    self.name,
                    Severity.ERROR,
                    f"{label}: inconsistent or undetermined binding",
                    _where(gadget),
                    error or f"readouts[{slot.position}] is not uniquely determined.",
                )
            else:
                yield from _zero_parity_diagnostic(
                    self.name,
                    gadget,
                    analysis,
                    analysis.readouts[slot.position],
                    label,
                    _equation(gadget.readouts[slot.position].equation),
                )


@dataclass(frozen=True)
class ActionMismatchRule:
    name: str = "gadget/action-mismatch"
    severity: Severity = Severity.ERROR
    phase: Phase = Phase.SEMANTIC
    target: type = qc.Gadget

    def __call__(self, target: object, *, qodec: qc.Qodec) -> Iterator[Diagnostic]:
        gadget = _gadget(target)
        if declaration_issues(gadget).unsupported_atoms:
            return
        try:
            expected = declared_action_of(gadget)
            actual = realized_action_of(gadget)
        except (KeyError, ValueError, TypeError, NotImplementedError) as error:
            if not gadget.inputs and gadget.outputs:
                yield Diagnostic(
                    self.name,
                    Severity.INFO,
                    "Logical action not checked: preparation has no input encoding",
                    _where(gadget),
                    f"{type(error).__name__}: {error}",
                )
                return
            yield Diagnostic(
                self.name,
                Severity.WARNING,
                "Logical action not checked: analysis failed",
                _where(gadget),
                f"{type(error).__name__}: {error}",
            )
            return
        if expected.is_equivalent_to(actual):
            return
        yield Diagnostic(
            self.name,
            self.severity,
            "circuit action differs from implements.action",
            _where(gadget),
            expected.why_not_equivalent_to(actual),
        )


@dataclass(frozen=True)
class ReadoutMismatchRule:
    name: str = "gadget/readout-mismatch"
    severity: Severity = Severity.ERROR
    phase: Phase = Phase.SEMANTIC
    target: type = qc.Gadget

    def __call__(self, target: object, *, qodec: qc.Qodec) -> Iterator[Diagnostic]:
        gadget = _gadget(target)
        try:
            mismatches = readout_disagreements(gadget)
        except (KeyError, ValueError, TypeError, NotImplementedError) as error:
            yield Diagnostic(
                self.name,
                Severity.WARNING,
                "Readouts not checked: analysis failed",
                _where(gadget),
                f"{type(error).__name__}: {error}",
            )
            return
        for mismatch in mismatches:
            position = mismatch.position
            declared = _equation(gadget.readouts[position].equation)
            detail = [f"Declared: {declared}"]
            if mismatch.expected_equation is not None:
                detail.append(
                    f"Verified relation: {_equation(mismatch.expected_equation)}"
                )
            detail.append(mismatch.reason)
            yield Diagnostic(
                self.name,
                self.severity,
                f"readouts[{position}] (logical {_observable(gadget, position)}): readout equation mismatch",
                _where(gadget),
                "\n".join(detail),
            )


@dataclass(frozen=True)
class IncompleteOutputFrameRule:
    name: str = "gadget/incomplete-output-frame"
    severity: Severity = Severity.WARNING
    phase: Phase = Phase.SEMANTIC
    target: type = qc.Gadget

    def __call__(self, target: object, *, qodec: qc.Qodec) -> Iterator[Diagnostic]:
        gadget = _gadget(target)
        try:
            missing = ParityAnalysis(gadget).unresolved_outputs()
        except (
            KeyError,
            ValueError,
            TypeError,
            AttributeError,
            NotImplementedError,
        ) as error:
            yield Diagnostic(
                self.name,
                Severity.WARNING,
                "Output frames not checked: analysis failed",
                _where(gadget),
                f"{type(error).__name__}: {error}",
            )
            return
        if not missing:
            return
        try:
            relations = _output_relations_of(gadget)
            unavailable = "No noiseless relation was derived."
        except (KeyError, ValueError, TypeError, NotImplementedError) as error:
            relations = []
            unavailable = f"Relation not derived: {type(error).__name__}: {error}"
        for path in missing:
            reference = qc.gadgets.Reference(path)
            operand, index = reference.entry, reference.index
            assert operand is not None
            sign = StabilizerSign("out", operand, index)
            encoding = gadget.outputs[operand]
            relation = next(
                (
                    (equation, offset)
                    for equation, offset in relations
                    if sign in equation
                ),
                None,
            )
            detail = unavailable
            if relation is not None:
                terms = _equation(
                    (sign, *(term for term in relation[0] if term != sign))
                )
                detail = (
                    f"Relation terms: {terms}\nParity: 1 (not a valid zero-parity check)."
                    if relation[1]
                    else f"Verified relation: {terms}"
                )
            yield Diagnostic(
                self.name,
                self.severity,
                f"Declared relations do not determine {sign} ({encoding.code.stabilizers[index]})",
                _where(gadget),
                f"Code: {encoding.code.name}; circuit support: {list(encoding.support)}.\n{detail}",
            )


RULES: tuple[Rule, ...] = (
    MissingCheckRule(),
    MissingObservableRule(),
    MissingFlagRule(),
    UnsupportedActionAtomRule(),
    CheckMismatchRule(),
    FlagMismatchRule(),
    ActionMismatchRule(),
    ReadoutMismatchRule(),
    IncompleteOutputFrameRule(),
)

__all__ = [
    "ActionMismatchRule",
    "CheckMismatchRule",
    "FlagMismatchRule",
    "IncompleteOutputFrameRule",
    "MissingCheckRule",
    "MissingFlagRule",
    "MissingObservableRule",
    "ReadoutMismatchRule",
    "RULES",
    "UnsupportedActionAtomRule",
]
